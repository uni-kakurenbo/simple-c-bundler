#include "message.h"
#include "message.inc"

void write_message(FILE *output, const char *name)
{
    asm_greeting(output, name);
}
