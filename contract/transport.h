#ifndef MUSIC_TRANSPORT_H
#define MUSIC_TRANSPORT_H
#include <stddef.h>
#include <stdint.h>

/* Synchronous calls copy bytes; neither side retains borrowed buffers.
 * Context and callbacks remain valid until muse_run returns.
 * submit: 0 accepted, 1 busy, 2 closed, 3 invalid length.
 * receive: 0 idle, -1 closed, positive required byte count; copy and consume
 * only when capacity is sufficient. No call blocks waiting for the other side.
 */
typedef struct {
    uint32_t abi_version;
    void *context;
    int32_t (*submit)(void *, const uint8_t *, size_t);
    int32_t (*receive)(void *, uint8_t *, size_t);
} MuseBridge;

int32_t muse_run(MuseBridge bridge, const uint8_t *options, size_t length);
#endif
