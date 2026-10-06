#include "unit.h"
#include "message.inc"
/* #include "does-not-exist.h"; static int false_positive(void); */
typedef struct State { int value; } State;
typedef int (*Callback)(void);
enum Kind { BASE = 4, INCREMENT = 1 };
static int first = 1, second[2] = {1, 2};
#define STEP(x) ((x) + INCREMENT)
static int value(void) { State s = {.value = BASE}; return STEP(s.value) + first + second[0]; }
static Callback callback = value;
static int indirect(void) { return callback(); }
const Module A = {.value = indirect, .show = asm_show};
