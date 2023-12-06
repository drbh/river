

build:
	docker buildx build \
	--platform linux/amd64 \
	-t registry.fly.io/yolov8-onnxruntime-rust:latest \
	-f Dockerfile .

run:
	docker run -it --rm \
	-p 3000:3000 \
	registry.fly.io/yolov8-onnxruntime-rust:latest

runshell:
	docker run -it --rm \
	-p 3000:3000 \
	--entrypoint /bin/bash \
	registry.fly.io/yolov8-onnxruntime-rust:latest

push:
	docker push registry.fly.io/yolov8-onnxruntime-rust:latest

# --push -t registry.fly.io/yolov8-onnxruntime-rust:latest \

# $3.89/mo
# [Model Preprocess]: 12.830019ms
# [ORT H2D]: 453.17µs
# [ORT Inference]: 380.286568ms
# [ORT D2H]: 251.949µs
# [Model Inference]: 381.112497ms
# [Model Postprocess]: 1.09302m
deploy:
	flyctl m run -a yolov8-onnxruntime-rust \
	--memory 512 \
	--cpus 2 \
	--region bos \
	-p 443:3000/tcp:http:tls \
	registry.fly.io/yolov8-onnxruntime-rust:latest 

# $7.78/mo
# [Model Preprocess]: 13.221299ms
# [ORT H2D]: 387.07µs
# [ORT Inference]: 156.535851ms
# [ORT D2H]: 377.24µs
# [Model Inference]: 157.426861ms
# [Model Postprocess]: 1.1601ms
deploy:
	flyctl m run -a yolov8-onnxruntime-rust \
	--memory 1024 \
	--cpus 4 \
	--region bos \
	-p 443:3000/tcp:http:tls \
	registry.fly.io/yolov8-onnxruntime-rust:latest 

# $15.55/mo
# [Model Preprocess]: 13.638869ms
# [ORT H2D]: 408.67µs
# [ORT Inference]: 139.355902ms
# [ORT D2H]: 313.46µs
# [Model Inference]: 140.201002ms
# [Model Postprocess]: 1.09189ms
deploy:
	flyctl m run -a yolov8-onnxruntime-rust \
	--memory 2048 \
	--cpus 8 \
	--region bos \
	-p 443:3000/tcp:http:tls \
	registry.fly.io/yolov8-onnxruntime-rust:latest 