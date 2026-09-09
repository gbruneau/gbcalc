# gbcalc -- TUI scientific calculator
#
#   make            build ./gbcalc
#   make test       build and run the engine unit tests
#   make run        build and launch
#   make themes     regenerate themes/default.conf from the built-in theme
#   make clean

CC      ?= cc
CFLAGS  ?= -O2 -std=c99 -Wall -Wextra -Wpedantic -Wshadow -Wconversion
LDLIBS  := -lm

PREFIX   ?= /usr/local
BINDIR   ?= $(PREFIX)/bin
THEMEDIR ?= $(PREFIX)/share/gbcalc/themes

CPPFLAGS += -DGBCALC_THEMEDIR='"$(THEMEDIR)"'

BIN     := gbcalc
SRC     := src/calc.c src/theme.c src/ui.c src/main.c
OBJ     := $(SRC:.c=.o)
HDR     := src/calc.h src/theme.h src/ui.h

TEST_BIN := tests/test_calc
TEST_SRC := tests/test_calc.c src/calc.c src/theme.c

.PHONY: all test run themes clean install uninstall

all: $(BIN)

$(BIN): $(OBJ)
	$(CC) $(CFLAGS) -o $@ $(OBJ) $(LDLIBS)

src/%.o: src/%.c $(HDR)
	$(CC) $(CFLAGS) $(CPPFLAGS) -c -o $@ $<

$(TEST_BIN): $(TEST_SRC) $(HDR)
	$(CC) $(CFLAGS) $(CPPFLAGS) -Isrc -o $@ $(TEST_SRC) $(LDLIBS)

test: $(TEST_BIN)
	./$(TEST_BIN)

run: $(BIN)
	./$(BIN)

# themes/default.conf is generated, so it can never drift from the binary.
themes: $(BIN)
	./$(BIN) --dump-theme > themes/default.conf

install: $(BIN)
	install -d $(DESTDIR)$(BINDIR) $(DESTDIR)$(THEMEDIR)
	install -m 755 $(BIN) $(DESTDIR)$(BINDIR)/$(BIN)
	install -m 644 themes/*.conf $(DESTDIR)$(THEMEDIR)/

uninstall:
	rm -f $(DESTDIR)$(BINDIR)/$(BIN)
	rm -rf $(DESTDIR)$(PREFIX)/share/gbcalc

clean:
	rm -f $(OBJ) $(BIN) $(TEST_BIN)
