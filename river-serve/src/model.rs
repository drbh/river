// use tokenizers::Tokenizer;
use anyhow::Result;
use std::sync::Arc;

use image::DynamicImage::ImageRgb8;
use openh264::decoder::Decoder;
use openh264::nal_units;
use serde_json::json;
use std::fs::File;
use std::io::Read;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tokio::sync::Notify;
use tokio::time::sleep;
use tokio::time::Duration;
use tokio::time::Instant;
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::{MediaEngine, MIME_TYPE_H264};
use webrtc::api::APIBuilder;
use webrtc::api::API;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::media::io::h264_writer::H264Writer;
use webrtc::media::io::Writer;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
use webrtc::rtp::packet::Packet;
use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability;
use webrtc::rtp_transceiver::rtp_codec::RTPCodecType;
use webrtc::track::track_local::track_local_static_rtp::TrackLocalStaticRTP;
use webrtc::track::track_local::{TrackLocal, TrackLocalWriter};
use webrtc::track::track_remote::TrackRemote;
use webrtc::Error;

use futures::{future, SinkExt, StreamExt};
use rand::Rng;
use std::rc::Rc;
use tmq::{dealer, router, Context, Multipart};

async fn client(
    // ctx: Rc<Context>,
    client_id: u64,
    frontend: String,
) -> tmq::Result<()> {
    let ctx = Rc::new(Context::new());
    let mut sock = dealer(&ctx).connect(&frontend)?;
    let mut rng = rand::thread_rng();

    let client_id = client_id.to_string();
    let mut request_id = 0;
    loop {
        println!("Client {} sending request {}", client_id, request_id);

        let request_id_str = request_id.to_string();
        let msg = vec![client_id.as_bytes(), request_id_str.as_bytes(), b"request"];
        sock.send(msg).await?;

        let response = sock.next().await.unwrap()?;
        let expected: Multipart =
            vec![client_id.as_bytes(), request_id_str.as_bytes(), b"response"].into();
        assert_eq!(expected, response);

        println!("Got response {:?}", response);
        let sleep_time = rng.gen_range(200..1000);
        sleep(Duration::from_millis(sleep_time)).await;
        request_id += 1;
    }
}

async fn save_to_disk(
    writer: Arc<Mutex<dyn webrtc::media::io::Writer + Send + Sync>>,
    track: Arc<TrackRemote>,
    notify: Arc<Notify>,
) -> Result<()> {
    loop {
        tokio::select! {
            result = track.read_rtp() => {
                if let Ok((rtp_packet, _)) = result {
                    let mut w = writer.lock().await;
                    w.write_rtp(&rtp_packet)?;
                }else{
                    println!("file closing begin after read_rtp error");
                    let mut w = writer.lock().await;
                    if let Err(err) = w.close() {
                        println!("file close err: {err}");
                    }
                    println!("file closing end after read_rtp error");
                    return Ok(());
                }
            }
            _ = notify.notified() => {
                println!("file closing begin after notified");
                let mut w = writer.lock().await;
                if let Err(err) = w.close() {
                    println!("file close err: {err}");
                }
                println!("file closing end after notified");
                return Ok(());
            }
        }
    }
}

// tokio RwLock

#[derive(Debug, Clone)]
pub struct ModelArgs {
    pub model: Option<String>,
    pub tokenizer: Option<String>,
    pub cpu: bool,
    pub quantized: bool,
}

pub struct ModelResources {
    pub api: API,
}

// Modify the TimedPacket to include the processing start time
struct TimedPacket {
    packet: Packet,
    processing_start_time: Instant,
    send_time: Instant,
}

impl ModelResources {
    pub async fn new(_args: ModelArgs) -> anyhow::Result<Self> {
        // let (model, tokenizer, logit_processor) = model(args)?;

        // Create a MediaEngine object to configure the supported codec
        let mut m = MediaEngine::default();

        m.register_codec(
            webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: MIME_TYPE_H264.to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "".to_owned(),
                    rtcp_feedback: vec![],
                },
                payload_type: 102,
                ..Default::default()
            },
            RTPCodecType::Video,
        )?;

        // Create a InterceptorRegistry. This is the user configurable RTP/RTCP Pipeline.
        // This provides NACKs, RTCP Reports and other features. If you use `webrtc.NewPeerConnection`
        // this is enabled by default. If you are manually managing You MUST create a InterceptorRegistry
        // for each PeerConnection.
        let mut registry = Registry::new();

        // Use the default set of Interceptors
        registry = register_default_interceptors(registry, &mut m)?;

        println!("Please enter the base64 encoded SessionDescription from the browser:");
        // Create the API object with the MediaEngine
        let api: API = APIBuilder::new()
            .with_media_engine(m)
            .with_interceptor_registry(registry)
            .build();

        // Prepare the configuration
        let _config = RTCConfiguration {
            ice_servers: vec![RTCIceServer {
                urls: vec!["stun:stun.l.google.com:19302".to_owned()],
                ..Default::default()
            }],
            ..Default::default()
        };

        Ok(Self { api })
    }

    pub async fn new_connection(
        &self,
        offer: String,
        config: RTCConfiguration,
    ) -> anyhow::Result<()> {
        //
        // Creae a new peer and init the connection
        //

        // Create a new RTCPeerConnection
        let peer_connection: Arc<RTCPeerConnection> =
            Arc::new(self.api.new_peer_connection(config).await?);
        let output_track = Arc::new(TrackLocalStaticRTP::new(
            RTCRtpCodecCapability {
                // mime_type: MIME_TYPE_VP8.to_owned(),
                mime_type: MIME_TYPE_H264.to_owned(),
                ..Default::default()
            },
            "video".to_owned(),
            "webrtc-rs".to_owned(),
        ));

        // Add this newly created track to the PeerConnection
        let rtp_sender = peer_connection
            .add_track(Arc::clone(&output_track) as Arc<dyn TrackLocal + Send + Sync>)
            .await?;

        // Read incoming RTCP packets
        // Before these packets are returned they are processed by interceptors. For things
        // like NACK this needs to be called.
        tokio::spawn(async move {
            let mut rtcp_buf = vec![0u8; 1500];
            while let Ok((_, _)) = rtp_sender.read(&mut rtcp_buf).await {}
            Result::<()>::Ok(())
        });

        // Set the remote SessionDescription from client (Offer)
        let desc_data = String::from_utf8(base64::decode(offer)?)?;
        let offer = serde_json::from_str::<RTCSessionDescription>(&desc_data)?;
        peer_connection.set_remote_description(offer).await?;

        //
        // Create a new track that we can write data to
        //

        // Which track is currently being handled
        let curr_track = Arc::new(AtomicUsize::new(0));
        // The total number of tracks
        let track_count = Arc::new(AtomicUsize::new(0));
        // The channel of packets with a bit of buffer
        let (packets_tx, mut packets_rx) =
            tokio::sync::mpsc::channel::<webrtc::rtp::packet::Packet>(60);
        let packets_tx = Arc::new(packets_tx);

        // Set a handler for when a new remote track starts, this handler copies inbound RTP packets,
        // replaces the SSRC and sends them back
        let pc = Arc::downgrade(&peer_connection);
        let curr_track1 = Arc::clone(&curr_track);
        let track_count1 = Arc::clone(&track_count);
        peer_connection.on_track(Box::new(move |track, _, _| {
            let track_num = track_count1.fetch_add(1, Ordering::SeqCst);

            let curr_track2 = Arc::clone(&curr_track1);
            let pc2 = pc.clone();
            let packets_tx2 = Arc::clone(&packets_tx);
            tokio::spawn(async move {
                println!(
                    "Track has started, of type {}: {}",
                    track.payload_type(),
                    track.codec().capability.mime_type
                );

                let mut last_timestamp = 0;
                let mut is_curr_track = false;
                while let Ok((mut rtp, _)) = track.read_rtp().await {
                    // Change the timestamp to only be the delta
                    let old_timestamp = rtp.header.timestamp;
                    if last_timestamp == 0 {
                        rtp.header.timestamp = 0
                    } else {
                        rtp.header.timestamp -= last_timestamp;
                    }
                    last_timestamp = old_timestamp;

                    // Check if this is the current track
                    if curr_track2.load(Ordering::SeqCst) == track_num {
                        // If just switched to this track, send PLI to get picture refresh
                        if !is_curr_track {
                            is_curr_track = true;
                            if let Some(pc) = pc2.upgrade() {
                                if let Err(err) = pc
                                    .write_rtcp(&[Box::new(PictureLossIndication {
                                        sender_ssrc: 0,
                                        media_ssrc: track.ssrc(),
                                    })])
                                    .await
                                {
                                    println!("write_rtcp err: {err}");
                                }
                            } else {
                                break;
                            }
                        }
                        let _ = packets_tx2.send(rtp).await;
                    } else {
                        is_curr_track = false;
                    }
                }

                println!(
                    "Track has ended, of type {}: {}",
                    track.payload_type(),
                    track.codec().capability.mime_type
                );
            });

            Box::pin(async {})
        }));

        //
        // Create listeners and channels so we can send packets to the browser
        //

        let (connected_tx, _connected_rx) = tokio::sync::mpsc::channel(1);
        let (done_tx, _done_rx) = tokio::sync::mpsc::channel(1);

        // Set the handler for Peer connection state
        // This will notify you when the peer has connected/disconnected
        peer_connection.on_peer_connection_state_change(Box::new(
            move |s: RTCPeerConnectionState| {
                println!("Peer Connection State has changed: {s}");
                if s == RTCPeerConnectionState::Connected {
                    let _ = connected_tx.try_send(());
                } else if s == RTCPeerConnectionState::Failed {
                    // Wait until PeerConnection has had no network activity for 30 seconds or another failure. It may be reconnected using an ICE Restart.
                    // Use webrtc.PeerConnectionStateDisconnected if you are interested in detecting faster timeout.
                    // Note that the PeerConnection may come back from PeerConnectionStateDisconnected.
                    let _ = done_tx.try_send(());
                }
                Box::pin(async move {})
            },
        ));

        //
        // Output data for client to make the connection
        //

        // Create an answer
        let answer = peer_connection.create_answer(None).await?;

        // Create channel that is blocked until ICE Gathering is complete
        let mut gather_complete = peer_connection.gathering_complete_promise().await;

        // Sets the LocalDescription, and starts our UDP listeners
        peer_connection.set_local_description(answer).await?;

        // Block until ICE Gathering is complete, disabling trickle ICE
        // we do this because we only can exchange one signaling message
        // in a production application you should exchange ICE Candidates via OnICECandidate
        let _ = gather_complete.recv().await;

        // Output the answer in base64 so we can paste it in browser
        if let Some(local_desc) = peer_connection.local_description().await {
            let json_str = serde_json::to_string(&local_desc)?;
            let b64 = base64::encode(json_str);
            println!("{b64}");
        } else {
            println!("generate local_description failed!");
        };

        // Define the capacity of the channels
        let (packet_tx, mut packet_rx) = mpsc::channel::<Packet>(1_00);
        let (processed_packet_tx, mut processed_packet_rx) = mpsc::channel::<TimedPacket>(32);

        let video_file = "output.h264";
        let h264_writer = Arc::new(Mutex::new(H264Writer::new(File::create(video_file)?)));
        let h264_writer2 = Arc::clone(&h264_writer);

        // Task for processing packets
        tokio::spawn(async move {
            // let ivf_writer3 = Arc::clone(&ivf_writer2);
            while let Some(packet) = packet_rx.recv().await {
                let h264_writer3 = Arc::clone(&h264_writer2);
                let processed_packet_tx_clone = processed_packet_tx.clone();
                let packet_clone = packet.clone();

                // write the rtp packet to disk and release the lock as soon as possible
                let mut w = h264_writer3.lock().await;
                match w.write_rtp(&packet_clone) {
                    Ok(_) => {
                        println!("write_rtp ok");
                    }
                    Err(err) => {
                        println!("write_rtp err: {err}");
                    }
                }
                drop(w);

                // read the file to vector of bytes
                let mut file = File::open(video_file).unwrap();
                let mut buffer = Vec::new();
                file.read_to_end(&mut buffer).unwrap();

                let mut decoder = Decoder::new().unwrap();
                let mut last_yuv = None;

                // Split H.264 into NAL units and decode each.
                for packet in nal_units(buffer.as_slice()) {
                    // On the first few frames this may fail, so you should check the result
                    // a few packets before giving up.
                    let result_some_yuv = decoder.decode(packet);

                    if let Ok(Some(yuv)) = result_some_yuv {
                        // Update the last decoded YUV frame
                        let mut buffer = vec![0; 200 * 200 * 3];
                        yuv.write_rgb8(&mut buffer);
                        last_yuv = Some(buffer.clone());
                    }
                }

                // Spawn a new task for processing each packet
                tokio::spawn(async move {
                    let processing_start_time = Instant::now();
                    println!("🌡️ PROCESS IT {}", packet.header.timestamp);

                    println!("packet size: {}", packet_clone.payload.len());

                    // Check if there was at least one successfully decoded frame
                    if let Some(yuv_buffer) = last_yuv {
                        let image =
                            ImageRgb8(image::RgbImage::from_raw(200, 200, yuv_buffer).unwrap());
                        let timestamped_name =
                            format!("data/output-{}.png", packet_clone.header.timestamp);

                        // b64 image
                        let mut buf = std::io::Cursor::new(Vec::new());
                        image
                            .write_to(&mut buf, image::ImageOutputFormat::Png)
                            .unwrap();
                        let b64_image = base64::encode(buf.into_inner());

                        let ctx = Arc::new(Context::new());
                        let frontend = "tcp://127.0.0.1:5555".to_string();

                        let mut sock = dealer(&ctx).connect(&frontend).unwrap();

                        let client_id = "1";
                        let request_id = packet_clone.header.timestamp.to_string();
                        println!("Client {} sending request {}", client_id, request_id);

                        let msg = vec![
                            client_id.as_bytes(),
                            request_id.as_bytes(),
                            b64_image.as_bytes(),
                        ];
                        sock.send(msg).await.unwrap();
                    } else {
                        println!("No frames were successfully decoded.");
                    }

                    let send_time = processing_start_time + Duration::from_millis(10_000);

                    // Send the processed packet to the processed packet channel
                    processed_packet_tx_clone
                        .send(TimedPacket {
                            packet,
                            processing_start_time,
                            send_time,
                        })
                        .await
                        .unwrap();
                });
            }
        });

        // Task for sending packets with 1-second total delay
        tokio::spawn(async move {
            let mut buffer: Vec<TimedPacket> = Vec::new();

            while let Some(timed_packet) = processed_packet_rx.recv().await {
                // Store the packet in the buffer
                buffer.push(timed_packet);

                // Check and send any packets that are due
                while let Some(first) = buffer.first() {
                    if first.send_time <= Instant::now() {
                        // If the packet's send time is due, send it
                        let packet_to_send = buffer.remove(0).packet;
                        println!("💸 SEND IT {}", packet_to_send.header.timestamp);

                        // Send it to the peer
                        if let Err(err) = output_track.write_rtp(&packet_to_send).await {
                            if Error::ErrClosedPipe == err {
                                // The peerConnection has been closed.
                                return;
                            } else {
                                panic!("{}", err);
                            }
                        }
                    } else {
                        // If the first packet in the buffer is not yet due, break the loop
                        break;
                    }
                }

                // Sleep a short duration to avoid busy-waiting
                sleep(Duration::from_millis(10)).await;
            }
        });

        // Main task for receiving and buffering packets
        tokio::spawn(async move {
            let mut curr_timestamp = 0;
            let mut i = 0;
            while let Some(mut packet) = packets_rx.recv().await {
                println!("👍 GOT IT {}", packet.header.timestamp);
                // Timestamp and sequence number processing
                curr_timestamp += packet.header.timestamp;
                packet.header.timestamp = curr_timestamp;
                packet.header.sequence_number = i;
                i += 1;
                // Send packet to the processing channel
                packet_tx.send(packet).await.unwrap();
                println!("After packet_tx.send");
            }
        });

        sleep(Duration::from_secs(1000)).await;

        peer_connection.close().await?;

        Ok(())
    }
}

pub async fn detect(
    _data: &mut ModelResources,
    _image: &[u8],
    _sender: tokio::sync::mpsc::Sender<std::string::String>,
) -> anyhow::Result<String> {
    Ok("hello".to_string())
}
