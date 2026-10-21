# Builds the x86_64 library preloaded into the game; OUT names where it goes.
OUT ?= ftlpad.so
CFLAGS := -O2 -Wall -Wextra -fPIC -fno-stack-protector -Isrc
SRC := $(wildcard src/*/*.c)
HDR := $(wildcard src/*.h src/*/*.h)

$(OUT): $(SRC) $(HDR)
	$(CC) $(CFLAGS) -shared -o $@ $(SRC) -lm

clean:
	rm -f $(OUT)

.PHONY: clean
