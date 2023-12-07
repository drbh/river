build:
	@docker build -t drbh/river .

run:
	@docker run -p 8080:8080 drbh/river