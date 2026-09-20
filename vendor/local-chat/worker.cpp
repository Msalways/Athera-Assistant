// Host-only worker: private prompt bytes arrive over stdin, never files or argv.
#include "athera_local_chat.h"
#include <iostream>
#include <iterator>
#include <string>

static int32_t emit(const char * bytes, int32_t length, void *) {
    std::cout.write(bytes, length).flush();
    return std::cout.good() ? 0 : 1;
}

int main(int argc, char ** argv) {
    if (argc != 3) return 2;
    try {
        const auto limit = std::stoul(argv[2]);
        if (limit == 0 || limit > 512) return 2;
        const std::string prompt((std::istreambuf_iterator<char>(std::cin)), {});
        if (prompt.size() > 64 * 1024) return 2;
        char error[256];
        auto * model = athera_local_model_load(argv[1], 4096, error, sizeof(error), nullptr, nullptr);
        if (model == nullptr) return 3;
        const auto result = athera_local_generate(model, prompt.c_str(), static_cast<uint32_t>(limit), emit, nullptr, nullptr);
        athera_local_model_unload(model);
        return result == 0 ? 0 : 4;
    } catch (...) { return 5; }
}
