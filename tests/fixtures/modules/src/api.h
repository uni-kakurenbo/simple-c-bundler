#ifndef FIXTURE_API_H
#define FIXTURE_API_H
#include <stdio.h>
typedef struct { int (*value)(void); void (*show)(FILE *, int); } Module;
#endif
