#!/bin/bash
export LD_LIBRARY_PATH=/usr/local/lib

./yolov8-rs --model yolov8m.onnx --source bike.jpeg --width 480 --height 640 --profile