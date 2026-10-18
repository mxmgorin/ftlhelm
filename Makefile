# Builds the x86_64 library the launcher preloads into the game through box64.
OUT := ../port/ftl/ftlpad.so
CFLAGS := -O2 -Wall -Wextra -fPIC -fno-stack-protector

$(OUT): ftlpad.c
	$(CC) $(CFLAGS) -shared -o $@ $< -lm

clean:
	rm -f $(OUT)

.PHONY: clean
