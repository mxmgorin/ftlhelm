# Builds the library with cargo and copies it to OUT, for scripts that call make.
OUT ?= ftlpad.so

$(OUT):
	cargo build --release
	cp target/release/libftlpad.so $(OUT)

test:
	cargo test

clean:
	cargo clean
	rm -f $(OUT)

.PHONY: $(OUT) test clean
