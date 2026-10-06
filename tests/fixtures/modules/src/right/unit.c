#include "unit.h"
#include "message.inc"
typedef struct State { int value; } State;
enum Kind { BASE = 9 };
static int value(void) { State s = {.value = BASE}; const State *p = &s; return p->value; }
const Module B = {.value = value, .show = asm_show};
