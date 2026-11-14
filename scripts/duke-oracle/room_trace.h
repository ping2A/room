/* Room sync-trace hook for Chocolate Duke3D (docs/duke-trace-format.md).
 * Drop into Game/src/ and call room_trace_after_tic() at the end of domovethings().
 * This file is Room's instrumentation only — it does not redistribute Build engine code.
 */
#ifndef ROOM_TRACE_H
#define ROOM_TRACE_H

#ifdef __cplusplus
extern "C" {
#endif

void room_trace_open(const char *path);
void room_trace_after_tic(void);
void room_trace_close(void);

#ifdef __cplusplus
}
#endif

#endif
