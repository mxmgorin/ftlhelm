# Builds the library with cargo and copies it to OUT, for scripts that call make, and the
# x86_64 probe program launchers run under box64 to check that the library loads.
OUT ?= ftlpad.so
PROBE ?= ftlpad_probe
# The archive a Linux player unpacks: the library, its layout and the launch wrapper.
DIST := ftlpad-linux-x86_64
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

dist: $(OUT)
	rm -rf dist/$(DIST)
	mkdir -p dist/$(DIST)
	cp $(OUT) config/ftlpad.ini tools/ftlpad.sh dist/$(DIST)/
	tar -C dist -czf dist/$(DIST).tar.gz $(DIST)

test:
	cargo test

lint:
	cargo fmt --check
	cargo clippy --release -- -D warnings

clean:
	cargo clean
	rm -rf $(OUT) $(PROBE) dist

.PHONY: all $(OUT) dist test lint clean
