# Builds the x86_64 library preloaded into the game; OUT names where it goes.
OUT ?= ftlpad.so
CFLAGS := -O2 -Wall -Wextra -fPIC -fno-stack-protector

$(OUT): ftlpad.c
	$(CC) $(CFLAGS) -shared -o $@ $< -lm

clean:
	rm -f $(OUT)

.PHONY: clean
