# Builds the library with cargo and copies it to OUT, for scripts that call make, and the
# x86_64 probe program launchers run under box64 to check that the library loads.
OUT ?= ftlpad.so
PROBE ?= ftlpad_probe
LIB := target/release/libftlpad.so
# Shared libraries box64 provides to an x86_64 preload; any other NEEDED entry fails to load.
ALLOWED_NEEDED := libc.so.6 libm.so.6 ld-linux-x86-64.so.2

all: $(OUT) $(PROBE)

$(OUT):
	cargo build --release
	@extra=$$(readelf -d $(LIB) | sed -n 's/.*(NEEDED).*\[\(.*\)\]/\1/p' | grep -vxF $(addprefix -e ,$(ALLOWED_NEEDED))); \
	if [ -n "$$extra" ]; then echo "$(LIB) needs libraries box64 lacks: $$extra" >&2; exit 1; fi
	cp $(LIB) $(OUT)

$(PROBE): tools/probe.c
	$(CC) -O1 -s -o $@ $<

test:
	cargo test

lint:
	cargo fmt --check
	cargo clippy --release -- -D warnings

clean:
	cargo clean
	rm -f $(OUT) $(PROBE)

.PHONY: all $(OUT) test lint clean
