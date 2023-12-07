use base64::{engine::general_purpose, Engine as _};
use futures::SinkExt;
use image::DynamicImage::ImageRgb8;
use openh264::{decoder::Decoder, nal_units};
use std::{fs::File, io::Read, sync::Arc};
use tmq::{dealer, Context};
use tokio::{
    sync::{mpsc, Mutex},
    time::{sleep, Duration, Instant},
};
use webrtc::{
    media::io::h264_writer::H264Writer,
    rtp::packet::Packet,
    track::{
        track_local::track_local_static_rtp::TrackLocalStaticRTP, track_local::TrackLocalWriter,
    },
    Error,
};

/// A struct to handle splitting of video frames from RTP packets.
pub struct FrameSplitter {}

struct TimedPacket {
    packet: Packet,
    send_time: Instant,

    // TODO: use to measure performance
    #[allow(dead_code)]
    processing_start_time: Instant,
    #[allow(dead_code)]
    processing_end_time: Instant,
}

impl FrameSplitter {
    /// Processes RTP packets, extracts video frames, and handles their transmission.
    pub fn process(
        output_track: Arc<TrackLocalStaticRTP>,
        packets_rx: mpsc::Receiver<Packet>,
    ) -> anyhow::Result<()> {
        let (packet_tx, packet_rx) = mpsc::channel::<Packet>(100);
        let (processed_packet_tx, processed_packet_rx) = mpsc::channel::<TimedPacket>(32);

        let video_file = "output.h264";
        let h264_writer = Arc::new(Mutex::new(H264Writer::new(File::create(video_file)?)));

        FrameSplitter::spawn_packet_processing_task(h264_writer, packet_rx, processed_packet_tx);
        FrameSplitter::spawn_packet_sending_task(output_track, processed_packet_rx);
        FrameSplitter::spawn_packet_receiving_task(packets_rx, packet_tx);

        Ok(())
    }

    fn spawn_packet_receiving_task(
        mut packets_rx: mpsc::Receiver<Packet>,
        packet_tx: mpsc::Sender<Packet>,
    ) {
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
    }

    fn spawn_packet_sending_task(
        output_track: Arc<TrackLocalStaticRTP>,
        mut processed_packet_rx: mpsc::Receiver<TimedPacket>,
    ) {
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
    }

    fn spawn_packet_processing_task(
        h264_writer: Arc<Mutex<dyn webrtc::media::io::Writer + Send + Sync>>,
        mut packet_rx: mpsc::Receiver<Packet>,
        processed_packet_tx: mpsc::Sender<TimedPacket>,
    ) {
        tokio::spawn(async move {
            // let ivf_writer3 = Arc::clone(&ivf_writer2);
            while let Some(packet) = packet_rx.recv().await {
                let h264_writer3 = Arc::clone(&h264_writer);
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

                let video_file = "output.h264";

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

                        let mut buf = std::io::Cursor::new(Vec::new());
                        image
                            .write_to(&mut buf, image::ImageOutputFormat::Png)
                            .unwrap();
                        let b64_image = general_purpose::STANDARD.encode(buf.into_inner());

                        // TODO: add a timeout so it doesn't seize up if no workers are available
                        if true {
                            let ctx = Arc::new(Context::new());
                            let frontend = "tcp://0.0.0.0:5555".to_string();

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
                        }
                    } else {
                        println!("No frames were successfully decoded.");
                    }

                    let send_time = processing_start_time + Duration::from_millis(3_000);

                    let processing_end_time = Instant::now();

                    // Send the processed packet to the processed packet channel
                    processed_packet_tx_clone
                        .send(TimedPacket {
                            packet,
                            send_time,
                            processing_start_time,
                            processing_end_time,
                        })
                        .await
                        .unwrap();
                });
            }
        });
    }
}
