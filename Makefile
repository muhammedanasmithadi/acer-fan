PREFIX ?= /usr/local
DESTDIR ?=

.PHONY: all build install uninstall

all: build

build:
	cargo build --release

install:
	sudo -E env "PATH=$$PATH" ./scripts/install-system.sh

uninstall:
	sudo ./scripts/uninstall.sh
