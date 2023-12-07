use anyhow::Result;
use base64::{engine::general_purpose, Engine as _};
use futures::{SinkExt, StreamExt};
use rand::Rng;
use std::{rc::Rc, sync::Arc};
use tmq::{dealer, Context, Multipart};
use tokio::{
    sync::{Mutex, Notify},
    time::{sleep, Duration},
};
use webrtc::api::{
    interceptor_registry::register_default_interceptors,
    media_engine::{MediaEngine, MIME_TYPE_H264},
    APIBuilder, API,
};
use webrtc::{
    ice_transport::ice_server::RTCIceServer,
    interceptor::registry::Registry,
    peer_connection::{
        configuration::RTCConfiguration, peer_connection_state::RTCPeerConnectionState,
    },
    rtp_transceiver::rtp_codec::{RTCRtpCodecCapability, RTPCodecType},
    track::track_remote::TrackRemote,
};

#[allow(dead_code)]
async fn client(client_id: u64, frontend: String) -> tmq::Result<()> {
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

#[allow(dead_code)]
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

#[derive(Debug, Clone)]
pub struct ModelArgs {
    pub model: Option<String>,
    pub tokenizer: Option<String>,
    pub cpu: bool,
    pub quantized: bool,
}

pub struct MediaStreamer {
    pub api: API,
}

impl MediaStreamer {
    pub async fn new() -> anyhow::Result<Self> {
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
        // Create a new RTCPeerConnection and grab a track from it
        let (peer_connection, output_track) =
            crate::river_webrtc::WebRTCService::create_new_peer(&self.api, config, offer).await?;

        // handle the incoming packets on that track and expose them via a channel
        let packets_rx =
            crate::river_webrtc::WebRTCService::handle_tracks(Arc::clone(&peer_connection)).await?;

        // Create listeners and channels so we can send packets to the browser
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

        // Output data for client to make the connection
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
            let b64 = general_purpose::STANDARD.encode(json_str);
            println!("{b64}");
        } else {
            println!("generate local_description failed!");
        };

        crate::frame_splitter::FrameSplitter::process(output_track, packets_rx)?;

        sleep(Duration::from_secs(1000)).await;

        peer_connection.close().await?;

        Ok(())
    }
}
