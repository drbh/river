build:
	@docker build -t drbh/river .

run:
	@docker run -p 3000:3000 -p 5555:5555 -p 5556:5556 drbh/river