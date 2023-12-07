# 🐟 river

`river` is a collection of lightweight services tailored for the efficient processing of video streams frame by frame, enabling near real-time inference.


⚠️ *This is an unfinished research project and does not gaurentee any level of stability or functionality.*

# 🐠 tldr 

`river-serve` and `river-zmq-proxy` consume a [h264](https://en.wikipedia.org/wiki/Advanced_Video_Coding) video stream via [webrtc](https://webrtc.org/), splits the stream into images and sends each to a [zmq](https://zeromq.org/) socket for inference. 

when processing a frame we offset the timestamp by a fixed amount of time. this offset creates a buffer period where we can begin to process the next frame before the previous frame has finished processing. this is a simple but effective way to reduce latency. additionally, the updated timestamps preserve the original timing of the frames so the reassembled video is in sync but with a slight delay 🙌

`river-onnx` is to be run on `N` number of machines; it consumes images from an inbound zmq socket, performs inference and sends the results to an outbound zmq socket. 

practically there would be many instances of `river-onnx`, and one instance of `river-serve` and `river-zmq-proxy`.

`river-zmq-proxy` is the critical link between `river-serve` and `river-onnx`. It's designed for non-blocking connections, enabling multiple threads to feed data to various workers simultaneously. Notably, it employs the [ROUTER/DEALER pattern](https://zguide.zeromq.org/docs/chapter3/#ROUTER-Broker-and-DEALER-Workers), which smartly routes messages to the next free worker, enhancing efficiency.

# 🐡 Services

| Service                                      | Description                                                                                                                 |
| -------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| [river-serve](river-serve/README.md)         | A small server that supports peer to peer WebRTC H264 video streaming                                                       |
| [river-onnx](river-onnx/README.md)           | A small server that supports inference of ONNX models                                                                       |
| [river-app](river-app/README.md)             | A simple web app that uses `river-serve` to stream video from a webcam and `river-onnx` to perform inference on each frame  |
| [river-zmq-proxy](river-zmq-proxy/README.md) | A small standalone application that handles connections between N number of DEALER sockets and a N number of ROUTER sockets |

# 🐬 Quickstart

this terminal is the video stream splitter and proxy services

```bash
# build docker with core services
make build

# run docker with core services
make run
```

this terminal is the inference service

```bash
cargo run --release --bin river-onnx
```

this terminal is the web app

```bash
cd river-app
bun dev
```