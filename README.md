# 🐟 river

`river` is a set of microservices for near real time video stream processing of frame by frame AI inference.

## Services

| Service | Description |
| --- | --- |
| [river-serve](river-serve/README.md) | A small server that supports peer to peer WebRTC H264 video streaming |
| [river-onnx](river-onnx/README.md) | A small server that supports inference of ONNX models |
| [river-app](river-app/README.md) | A simple web app that uses `river-serve` to stream video from a webcam and `river-onnx` to perform inference on each frame |


## General Concept

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
