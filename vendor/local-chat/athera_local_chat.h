#pragma once

#include <stdint.h>

#if defined(_WIN32)
#if defined(ATHERA_BUILD_SHARED)
#define ATHERA_API __declspec(dllexport)
#else
#define ATHERA_API __declspec(dllimport)
#endif
#else
#define ATHERA_API __attribute__((visibility("default")))
#endif

#ifdef __cplusplus
extern "C" {
#endif

typedef struct athera_local_model athera_local_model;
typedef int32_t (*athera_token_callback)(const char * bytes, int32_t length, void * user_data);
typedef int32_t (*athera_cancel_callback)(void * user_data);

// The caller serializes generation and owns the returned model until unload.
ATHERA_API athera_local_model * athera_local_model_load(
    const char * model_path,
    uint32_t context_tokens,
    char * error,
    uint32_t error_capacity,
    athera_cancel_callback cancel_callback,
    void * user_data);

// Returns 0 on success, 1 on cancellation, and a negative value on failure.
ATHERA_API int32_t athera_local_generate(
    athera_local_model * model,
    const char * prompt,
    uint32_t max_output_tokens,
    athera_token_callback token_callback,
    athera_cancel_callback cancel_callback,
    void * user_data);

ATHERA_API void athera_local_model_unload(athera_local_model * model);

#ifdef __cplusplus
}
#endif
