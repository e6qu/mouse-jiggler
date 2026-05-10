COSMOCC ?= ../tools/cosmocc/bin/cosmocc
CFLAGS  ?= -Os -Wall -Wextra -Wno-unused-parameter
LDFLAGS ?= -lm

BIN := mouse-jiggler

.PHONY: all clean run

all: $(BIN)

$(BIN): main.c
	$(COSMOCC) $(CFLAGS) -o $@ $< $(LDFLAGS)

clean:
	rm -f $(BIN) $(BIN).aarch64.elf $(BIN).com.dbg

run: $(BIN)
	./$(BIN) --once -m pixel -d 3 -v
