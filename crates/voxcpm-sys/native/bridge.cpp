#include "voxcpm2_runtime.h"
#include "ggml-backend.h"
#include <atomic>
#include <memory>
#include <exception>
#include <cstring>

struct TrnVox {
    VoxCPM2Runtime runtime;
    std::atomic<bool> cancelled{false};
    std::string error;
};
static thread_local std::string creation_error;
using AudioCallback = bool (*)(void *, const float *, size_t, bool);

extern "C" {
bool trn_vox_available(int device) {
    if (device == 0) return true;
    const char * expected = device == 1 ? "CUDA" : device == 2 ? "Metal" : "";
    for (size_t i = 0; i < ggml_backend_dev_count(); ++i) {
        auto dev = ggml_backend_dev_get(i);
        if (std::strcmp(ggml_backend_reg_name(ggml_backend_dev_backend_reg(dev)), expected) == 0) return true;
    }
    return false;
}
TrnVox * trn_vox_create(const char * base, const char * acoustic, int device) {
    try {
        if (!trn_vox_available(device)) { creation_error = "requested device is unavailable"; return nullptr; }
        auto model = std::make_unique<TrnVox>();
        if (!model->runtime.init(base, acoustic, device == 0 ? 0 : -1, device != 0)) {
            creation_error = model->runtime.last_error(); return nullptr;
        }
        const char * backend = model->runtime.backend_name();
        if ((device == 1 && !std::strstr(backend, "CUDA")) || (device == 2 && !std::strstr(backend, "Metal"))) {
            creation_error = "explicit GPU request fell back to another backend"; return nullptr;
        }
        return model.release();
    } catch (const std::exception & e) { creation_error = e.what(); return nullptr; }
    catch (...) { creation_error = "unknown native initialization error"; return nullptr; }
}
void trn_vox_destroy(TrnVox * model) { delete model; }
void trn_vox_cancel(TrnVox * model) { model->cancelled.store(true); }
const char * trn_vox_error(const TrnVox * model) { return model ? model->error.c_str() : creation_error.c_str(); }
bool trn_vox_encode(TrnVox * model, const float * samples, size_t length, int rate, AudioCallback callback, void * context) {
    try {
        auto features = model->runtime.encode_reference_audio(std::vector<float>(samples, samples + length), rate);
        if (features.empty()) { model->error = model->runtime.last_error(); return false; }
        return callback(context, features.data(), features.size(), true);
    } catch (const std::exception & e) { model->error = e.what(); return false; }
    catch (...) { model->error = "unknown native reference encoding error"; return false; }
}
// 0=normal EOS, 1=cancelled, 2=frame limit, 3=error. Only 0 completes a segment.
int trn_vox_generate(TrnVox * model, const char * text, const float * reference, size_t reference_len, int reference_rate, bool encoded, const char * transcript, int max_steps, AudioCallback callback, void * context) {
    try {
        model->cancelled.store(false);
        bool ended = false;
        VoxCPM2GenerateParams params;
        params.max_steps = max_steps;
        params.seed = 42;
        params.reference_sample_rate = reference_rate;
        auto emit = [&](const std::vector<float> & pcm, bool final) {
            if (model->cancelled.load()) return false;
            if (!callback(context, pcm.data(), pcm.size(), final)) { model->cancelled.store(true); return false; }
            ended = ended || final;
            return true;
        };
        bool ok;
        if (reference && reference_len) {
            std::vector<float> wav(reference, reference + reference_len);
            if (encoded) ok = model->runtime.generate_with_features_streaming(text, transcript, wav, emit, params);
            else if (transcript && *transcript) ok = model->runtime.generate_with_continuation_streaming(text, transcript, wav, emit, params);
            else ok = model->runtime.generate_with_clone_streaming(text, wav, emit, params);
        } else ok = model->runtime.generate_streaming(text, emit, params);
        if (model->cancelled.load()) return 1;
        if (!ok) { model->error = model->runtime.last_error(); return 3; }
        return ended ? 0 : 2;
    } catch (const std::exception & e) { model->error = e.what(); return 3; }
    catch (...) { model->error = "unknown native synthesis error"; return 3; }
}
}
