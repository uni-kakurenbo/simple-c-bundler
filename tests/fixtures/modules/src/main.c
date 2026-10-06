#include "left/unit.h"
#include "right/unit.h"
#if defined(value) || defined(State) || defined(asm_show) || defined(STEP)
#error Private names leaked out of their module
#endif
int main(void) {
    printf("value State %d %d\n", A.value(), B.value());
    A.show(stdout, 42);
    B.show(stdout, 11);
    return 0;
}
