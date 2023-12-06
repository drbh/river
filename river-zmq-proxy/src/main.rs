use futures::{future, SinkExt, StreamExt};
use rand::Rng;
use std::{error::Error, rc::Rc, time::Duration};
use tmq::{dealer, router, Context, Multipart};
use tokio::time::sleep;

/// Simulates zmq::proxy using asynchronous sockets.
async fn proxy(ctx: Rc<Context>, frontend: String, backend: String) -> tmq::Result<()> {
    let (mut router_tx, mut router_rx) = router(&ctx).bind(&frontend)?.split();
    let (mut dealer_tx, mut dealer_rx) = dealer(&ctx).bind(&backend)?.split();

    let mut frontend_fut = router_rx.next();
    let mut backend_fut = dealer_rx.next();

    loop {
        let msg = future::select(frontend_fut, backend_fut).await;
        match msg {
            future::Either::Left(router_msg) => {
                // proxy received a message from a client
                dealer_tx.send(router_msg.0.unwrap()?).await?;
                frontend_fut = router_rx.next();
                backend_fut = router_msg.1;
            }
            future::Either::Right(dealer_msg) => {
                // proxy received a message from a worker
                router_tx.send(dealer_msg.0.unwrap()?).await?;
                backend_fut = dealer_rx.next();
                frontend_fut = dealer_msg.1;
            }
        }
    }
}

fn main() -> tmq::Result<()> {
    let frontend = "tcp://127.0.0.1:5555".to_string();
    let backend = "tcp://127.0.0.1:5556".to_string();
    let ctx = Rc::new(Context::new());

    let mut runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let tasks = tokio::task::LocalSet::new();

    tasks.block_on(&mut runtime, async move {
        proxy(ctx.clone(), frontend, backend)
            .await
            .expect("Proxy failed");
    });

    Ok(())
}