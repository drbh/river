# 🐟 river

`river` is a set of microservices for near real time video stream processing of frame by frame AI inference.

## Services

| Service | Description |
| --- | --- |
| [river-serve](river-serve/README.md) | A small server that supports peer to peer WebRTC H264 video streaming |
| [river-onnx](river-onnx/README.md) | A small server that supports inference of ONNX models |
| [river-app](river-app/README.md) | A simple web app that uses `river-serve` to stream video from a webcam and `river-onnx` to perform inference on each frame |


## General Concepts

### Video Fragmentation

`river-serve` is a small server that supports peer to peer WebRTC H264 video streaming, as each video fragment is received it's placed on the `packet` channel. 

A second thread reads from the `packet` channel and reconstructs the most recent video frame and decodes it into a `image.Image` object. 

A third thread then sends the `image.Image` object to a `river-onnx` service for inference via HTTP POST, awaits the response. Then we offset the original packet timestamp by N (3 seconds) amount of time and finally send the response and original fragment to the `processed_packet` channel.

A fourth thread reads from the `processed_packet` and sends the fragment to the peer.

The result of this is a output stream that is delayed by N amount of time, but has inference results for each frame and as long as the peer can keep up with the delay it will be a near real time stream with inference results.


# Engineering challenges

- 1. How to reconstruct a video frame from a series of fragmented H264 packets
- 2. How to perform inference on each frame
- 3. How to send the inference results back to the peer
- 4. How to keep the peer in sync with the delay

### Scalable AI Inference

`river-onnx` is a small ZMQ server that supports inference of ONNX models. It's designed to be horizontally scalable and is stateless.

It can be thought of as a simple microservice that waits for ZMQ messages and performs inference on the message payload and returns the result. 

The ZMQ socket is of type DEALER and when used in tandem with a small PROXY server it can be scaled horizontally to support multiple instances of the service.

This patern is implemented with the help of the `river-zmq-proxy` package. This package is a small standalone application that handles connections between N number of DEALER sockets and a N number of ROUTER sockets. 

The result of this is the ability to deploy multiple instances of `river-onnx` and have them scale horizontally to support the load. 

On each image `river-server` sends a ZMQ message to the `river-zmq-proxy` with the payload being the image bytes. The `river-zmq-proxy` then forwards the message to one of the `river-onnx` instances. The `river-onnx` instance performs inference and sends the result back to the `river-zmq-proxy` which then forwards the result back to `river-serve` which then sends the result back to the peer.

The advantage of this approach is that the proxy will always send the message to the least busy `river-onnx` instance, and lets us roughtly distribute the load evenly across all instances even if the instances have different hardware specs and inference times.

