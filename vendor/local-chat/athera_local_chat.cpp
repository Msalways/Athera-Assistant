#include "athera_local_chat.h"
#include "llama.h"

#include <algorithm>
#include <cstring>
#include <mutex>
#include <memory>
#include <string>
#include <vector>

struct athera_local_model {
    llama_model * model;
    uint32_t context_tokens;
};

static void copy_error(char * output, uint32_t capacity, const char * message) {
    if (output == nullptr || capacity == 0) return;
    const size_t length = std::min<size_t>(capacity - 1, std::strlen(message));
    std::memcpy(output, message, length);
    output[length] = '\0';
}

extern "C" ATHERA_API athera_local_model * athera_local_model_load(
    const char * model_path,
    uint32_t context_tokens,
    char * error,
    uint32_t error_capacity,
    athera_cancel_callback cancel_callback,
    void * user_data) try {
    if (model_path == nullptr || context_tokens == 0) {
        copy_error(error, error_capacity, "invalid model configuration");
        return nullptr;
    }
    static std::once_flag backend;
    std::call_once(backend, llama_backend_init);
    llama_model_params params = llama_model_default_params();
    params.n_gpu_layers = 0;
    struct load_cancel { athera_cancel_callback callback; void * data; } cancellation { cancel_callback, user_data };
    params.progress_callback = [](float, void * opaque) {
        const auto * value = static_cast<load_cancel *>(opaque);
        return value->callback == nullptr || value->callback(value->data) == 0;
    };
    params.progress_callback_user_data = &cancellation;
    std::unique_ptr<llama_model, decltype(&llama_model_free)> model(llama_model_load_from_file(model_path, params), llama_model_free);
    if (model == nullptr) {
        copy_error(error, error_capacity, "llama.cpp could not load the GGUF model");
        return nullptr;
    }
    auto * result = new athera_local_model { model.get(), context_tokens };
    model.release();
    return result;
} catch (...) {
    copy_error(error, error_capacity, "native model loading failed");
    return nullptr;
}

extern "C" ATHERA_API int32_t athera_local_generate(
    athera_local_model * local,
    const char * prompt,
    uint32_t max_output_tokens,
    athera_token_callback token_callback,
    athera_cancel_callback cancel_callback,
    void * user_data) try {
    if (local == nullptr || prompt == nullptr || token_callback == nullptr || max_output_tokens == 0) return -1;
    const llama_vocab * vocab = llama_model_get_vocab(local->model);
    const int32_t prompt_bytes = static_cast<int32_t>(std::strlen(prompt));
    const int32_t token_count = -llama_tokenize(vocab, prompt, prompt_bytes, nullptr, 0, true, true);
    if (token_count <= 0 || static_cast<uint64_t>(token_count) + max_output_tokens > local->context_tokens) return -2;
    std::vector<llama_token> tokens(token_count);
    if (llama_tokenize(vocab, prompt, prompt_bytes, tokens.data(), token_count, true, true) != token_count) return -3;

    llama_context_params context_params = llama_context_default_params();
    context_params.n_ctx = local->context_tokens;
    context_params.n_batch = std::min<uint32_t>(local->context_tokens, static_cast<uint32_t>(token_count));
    context_params.no_perf = true;
    struct generation_cancel { athera_cancel_callback callback; void * data; } cancellation { cancel_callback, user_data };
    context_params.abort_callback = [](void * opaque) {
        const auto * value = static_cast<generation_cancel *>(opaque);
        return value->callback != nullptr && value->callback(value->data) != 0;
    };
    context_params.abort_callback_data = &cancellation;
    std::unique_ptr<llama_context, decltype(&llama_free)> context_owner(llama_init_from_model(local->model, context_params), llama_free);
    llama_context * context = context_owner.get();
    if (context == nullptr) return -4;

    llama_sampler_chain_params chain_params = llama_sampler_chain_default_params();
    chain_params.no_perf = true;
    std::unique_ptr<llama_sampler, decltype(&llama_sampler_free)> sampler_owner(llama_sampler_chain_init(chain_params), llama_sampler_free);
    llama_sampler * sampler = sampler_owner.get();
    if (sampler == nullptr) return -4;
    llama_sampler_chain_add(sampler, llama_sampler_init_top_k(20));
    llama_sampler_chain_add(sampler, llama_sampler_init_top_p(0.8F, 1));
    llama_sampler_chain_add(sampler, llama_sampler_init_min_p(0.0F, 1));
    llama_sampler_chain_add(sampler, llama_sampler_init_temp(0.7F));
    llama_sampler_chain_add(sampler, llama_sampler_init_dist(LLAMA_DEFAULT_SEED));

    int32_t result = 0;
    llama_batch batch = llama_batch_get_one(tokens.data(), token_count);
    llama_token next_token = LLAMA_TOKEN_NULL;
    for (uint32_t generated = 0; generated < max_output_tokens; ++generated) {
        if (cancel_callback != nullptr && cancel_callback(user_data) != 0) {
            result = 1;
            break;
        }
        if (llama_decode(context, batch) != 0) {
            result = cancel_callback != nullptr && cancel_callback(user_data) != 0 ? 1 : -5;
            break;
        }
        next_token = llama_sampler_sample(sampler, context, -1);
        if (llama_vocab_is_eog(vocab, next_token)) break;
        char stack_buffer[256];
        int32_t length = llama_token_to_piece(vocab, next_token, stack_buffer, sizeof(stack_buffer), 0, true);
        std::vector<char> heap_buffer;
        const char * bytes = stack_buffer;
        if (length < 0) {
            heap_buffer.resize(static_cast<size_t>(-length));
            length = llama_token_to_piece(vocab, next_token, heap_buffer.data(), heap_buffer.size(), 0, true);
            bytes = heap_buffer.data();
        }
        if (length < 0 || token_callback(bytes, length, user_data) != 0) {
            result = -6;
            break;
        }
        batch = llama_batch_get_one(&next_token, 1);
    }
    return result;
} catch (...) {
    return -7;
}

extern "C" ATHERA_API void athera_local_model_unload(athera_local_model * local) {
    if (local == nullptr) return;
    llama_model_free(local->model);
    delete local;
}
