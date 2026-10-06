#pragma once
#include <stdio.h>
typedef struct { int (*value)(void); void (*show)(FILE *, int); } Module;
