use poem::http::StatusCode;
use poem::{
    get, handler, listener::TcpListener, middleware::Tracing, post, web::Data, web::Path,
    EndpointExt, Route, Server,
};
use poem::{
    web::{Html, Json},
    Response,
};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

mod frame_splitter;
mod media_streamer;
mod river_webrtc;

#[derive(Debug, Deserialize)]
struct CreateSomething {
    name: String,
}

// Shared data between requests (it holds a mutex to the model)
#[derive(Clone)]
struct Shared {
    media_streamer: Arc<Mutex<media_streamer::MediaStreamer>>,
}

impl Shared {
    async fn new() -> Self {
        Self {
            media_streamer: Arc::new(Mutex::new(
                media_streamer::MediaStreamer::new().await.unwrap(),
            )),
        }
    }
}

pub async fn send_and_wait(sender: mpsc::Sender<String>, msg: String) {
    sender.send(msg).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(1)).await;
}

#[handler]
async fn index() -> Html<String> {
    let filename = "index.html";
    let directory = "dist";
    let cwd = std::env::current_dir().unwrap();
    let cwd_as_str = cwd.to_str().unwrap();
    let full_pattern = format!("{}/app/{}/{}", cwd_as_str, directory, filename);
    println!("full pattern: --{}--", full_pattern);

    let contents = tokio::fs::read_to_string(full_pattern)
        .await
        .unwrap_or_else(|_| "".to_string());
    Html(contents)
}

// get css or js files assets
#[handler]
async fn assets(Path(filename): Path<String>) -> Response {
    if !filename.starts_with("index-") {
        let response = Response::from(StatusCode::NOT_FOUND);
        return response;
    }

    let directory = "dist/assets";
    let cwd = std::env::current_dir().unwrap();
    let cwd_as_str = cwd.to_str().unwrap();
    let full_pattern = format!("{}/app/{}/{}", cwd_as_str, directory, filename);

    let contents = std::fs::read_to_string(full_pattern).unwrap();
    match filename.ends_with(".js") {
        true => {
            let mut response = Response::from(contents);
            // set mime type
            response
                .headers_mut()
                .insert("content-type", "application/javascript".parse().unwrap());
            response
        }
        false => {
            let mut response = Response::from(contents);
            // set mime type
            response
                .headers_mut()
                .insert("content-type", "text/css".parse().unwrap());
            response
        }
    }
}

#[handler]
async fn offer(req: Json<CreateSomething>, shared: Data<&Shared>) -> Html<String> {
    let name = req.name.clone();
    let shared2 = shared.clone();
    // call new_connection on the model
    let mut shared_model = shared2.media_streamer.lock().await;
    let offer = shared_model
        .new_connection(
            name,
            webrtc::peer_connection::configuration::RTCConfiguration {
                ice_servers: vec![webrtc::ice_transport::ice_server::RTCIceServer {
                    urls: vec!["stun:stun.l.google.com:19302".to_owned()],
                    ..Default::default()
                }],
                ..Default::default()
            },
        )
        .await
        .unwrap();
    drop(shared_model); // unlock the mutex
    println!("offer: {:?}", offer);
    Html("woop".to_string())
}

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    if std::env::var_os("RUST_LOG").is_none() {
        std::env::set_var("RUST_LOG", "poem=debug");
    }
    tracing_subscriber::fmt::init();
    let shared_data = Shared::new().await;
    let new_app = || {
        Route::new()
            .at("/", get(index))
            .at("/assets/:filename", get(assets))
            .at("/offer", post(offer))
            .data(shared_data)
            .with(Tracing)
    };
    Server::new(TcpListener::bind("0.0.0.0:3000"))
        .name("river-serve")
        .run(new_app())
        .await
}
