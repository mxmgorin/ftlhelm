# Builds the library into OUT, and the x86_64 probe that shows whether it loads under box64.
OUT ?= ftlhelm.so
PROBE ?= ftlhelm_probe
# The archive a Linux player unpacks: the library, its layout and the launch wrapper.
DIST := ftlhelm-linux-x86_64
# The library's directory in the archive, apart from the files ftlhelm writes.
DIST_LIB := $(DIST)/lib
LIB := target/release/libftlhelm.so
# Shared libraries box64 provides to an x86_64 preload; any other NEEDED entry fails to load.
ALLOWED_NEEDED := libc.so.6 libm.so.6 ld-linux-x86-64.so.2
# The FTL port for PortMaster: packaging/portmaster around the library and box64.
PORT := dist/ftlhelm-portmaster.zip
# The box64 the Ittle Dew port ships, pinned by PortMaster-New commit and checksum.
BOX64 ?= dist/box64
BOX64_URL := https://raw.githubusercontent.com/PortsMaster/PortMaster-New/2cbd59830877df346a7f4dd3a62c17e8385eea52/ports/ittledew/ittledew/box64
BOX64_SHA256 := 266d0aa3bc1e2d597df7c08f749dbcbcb2da6885d32f30e902bd2dd71b92e760

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
	mkdir -p dist/$(DIST_LIB)
	cp $(OUT) dist/$(DIST_LIB)/
	cp config/ftlhelm.ini tools/ftlhelm.sh LICENSE NOTICE TRADEMARKS.md dist/$(DIST)/
	tar -C dist -czf dist/$(DIST).tar.gz $(DIST)

portmaster: $(OUT) $(BOX64)
	python3 tools/mkzip.py --lib $(OUT) --box64 $(BOX64) $(PORT)

$(BOX64):
	mkdir -p $(@D)
	curl -fsSL -o $@.part $(BOX64_URL)
	echo "$(BOX64_SHA256)  $@.part" | sha256sum -c --quiet
	mv $@.part $@

test:
	cargo test

lint:
	cargo fmt --check
	cargo clippy --release -- -D warnings

clean:
	cargo clean
	rm -rf $(OUT) $(PROBE) dist

.PHONY: all $(OUT) dist portmaster test lint clean
