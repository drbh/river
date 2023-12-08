use base64::{engine::general_purpose, Engine as _};
use futures::{future, SinkExt, StreamExt};
use rand::Rng;
use river_onnx::{Args, YOLOTask, YOLOv8};
use std::sync::Arc;
use std::{error::Error, rc::Rc};
use tmq::{dealer, Context};
use tokio::sync::Mutex;

type SharedModel = Arc<Mutex<YOLOv8>>;

async fn worker(
    ctx: Rc<Context>,
    worker_id: u64,
    backend: String,
    model: SharedModel,
) -> Result<(), Box<dyn Error>> {
    let mut sock = dealer(&ctx).connect(&backend)?;

    println!("Worker {} starting", worker_id);

    loop {
        let mut request = sock.next().await.unwrap()?;
        let identity = request.pop_front().unwrap();
        let client_id = request.pop_front().unwrap();
        let request_id = request.pop_front().unwrap();
        let request_body = request.pop_front().unwrap();

        println!(
            "Worker {} handling request(id={} body=...) from client {}",
            worker_id,
            request_id.as_str().unwrap(),
            client_id.as_str().unwrap()
        );

        let b64_image = request_body.as_str().unwrap();

        let image = general_purpose::STANDARD.decode(b64_image).unwrap();

        // decode base64 image
        // let image = base64::decode(b64_image).unwrap();
        let image = image::load_from_memory(&image).unwrap();

        // 2. model support dynamic batch inference, so input should be a Vec
        let xs = vec![image];

        // Access the shared model
        let mut model = model.lock().await;

        model.summary(); // model info

        // // 3. build yolov8 model
        // let mut model = YOLOv8::new(args).unwrap();
        // model.summary(); // model info

        // 4. run
        let ys = model.run(&xs).unwrap();
        println!("{:?}", ys);

        // Important: release the lock before sending response
        drop(model);

        let response = vec![identity, client_id, request_id, "response".into()];
        sock.send(response).await?;
    }
}

fn main() -> tmq::Result<()> {
    let backend = "tcp://127.0.0.1:5556".to_string();
    let ctx = Rc::new(Context::new());

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let tasks = tokio::task::LocalSet::new();

    let args = Args {
        model: String::from("river-onnx/yolov8m.onnx"),
        source: String::from(""),
        device_id: 0,
        trt: false,
        cuda: false,
        batch: 1,
        batch_min: 1,
        batch_max: 32,
        fp16: false,
        task: Some(YOLOTask::Detect),
        nc: None,
        nk: None,
        nm: None,
        width: Some(480),
        height: Some(640),
        conf: 0.3,
        iou: 0.45,
        kconf: 0.55,
        plot: false,
        profile: true,
    };

    let model = Arc::new(Mutex::new(YOLOv8::new(args).unwrap()));

    // spawn worker
    tasks.spawn_local(async move {
        let randome_worker_id = rand::thread_rng().gen_range(10_000..100_000);
        worker(ctx, randome_worker_id, backend, model)
            .await
            .expect("Worker failed");
    });

    // block on all tasks
    tasks.block_on(&runtime, async move {
        future::pending::<()>().await;
    });

    Ok(())
}
