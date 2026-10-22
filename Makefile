# Builds the library with cargo and copies it to OUT, for scripts that call make.
OUT ?= ftlpad.so
LIB := target/release/libftlpad.so
# Shared libraries box64 provides to an x86_64 preload; any other NEEDED entry fails to load.
ALLOWED_NEEDED := libc.so.6 libm.so.6 ld-linux-x86-64.so.2

$(OUT):
	cargo build --release
	@extra=$$(readelf -d $(LIB) | sed -n 's/.*(NEEDED).*\[\(.*\)\]/\1/p' | grep -vxF $(addprefix -e ,$(ALLOWED_NEEDED))); \
	if [ -n "$$extra" ]; then echo "$(LIB) needs libraries box64 lacks: $$extra" >&2; exit 1; fi
	cp $(LIB) $(OUT)

test:
	cargo test

lint:
	cargo fmt --check
	cargo clippy --release -- -D warnings

clean:
	cargo clean
	rm -f $(OUT)

.PHONY: $(OUT) test lint clean
