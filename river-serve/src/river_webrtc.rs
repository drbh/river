// use tokenizers::Tokenizer;
use anyhow::Result;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use webrtc::api::media_engine::MIME_TYPE_H264;
use webrtc::api::API;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability;
use webrtc::track::track_local::track_local_static_rtp::TrackLocalStaticRTP;
use webrtc::track::track_local::TrackLocal;
use base64::{engine::general_purpose, Engine as _};

pub struct WebRTCService {}

impl WebRTCService {
    pub async fn create_new_peer(
        api: &API,
        config: RTCConfiguration,
        offer: String,
    ) -> anyhow::Result<(Arc<RTCPeerConnection>, Arc<TrackLocalStaticRTP>)> {
        // Create a new RTCPeerConnection
        let peer_connection: Arc<RTCPeerConnection> =
            Arc::new(api.new_peer_connection(config).await?);
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
        let desc_data = String::from_utf8(general_purpose::STANDARD.decode(offer)?)?;
        let offer = serde_json::from_str::<RTCSessionDescription>(&desc_data)?;
        peer_connection.set_remote_description(offer).await?;
        Ok((peer_connection, output_track))
    }

    pub async fn handle_tracks(
        peer_connection: Arc<RTCPeerConnection>,
    ) -> anyhow::Result<tokio::sync::mpsc::Receiver<webrtc::rtp::packet::Packet>> {
        // Which track is currently being handled
        let curr_track = Arc::new(AtomicUsize::new(0));
        // The total number of tracks
        let track_count = Arc::new(AtomicUsize::new(0));
        // The channel of packets with a bit of buffer
        let (packets_tx, packets_rx) =
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

        Ok(packets_rx)
    }
}
