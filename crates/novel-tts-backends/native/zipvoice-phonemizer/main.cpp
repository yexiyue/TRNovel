#include <espeak-ng/speak_lib.h>
#include <iostream>
#include <string>
int main() {
    if (espeak_Initialize(AUDIO_OUTPUT_SYNCHRONOUS, 0, ".", espeakINITIALIZE_DONT_EXIT) < 0) return 1;
    if (espeak_SetVoiceByName("en-us") != EE_OK) return 2;
    std::string text;
    std::getline(std::cin, text, '\0');
    const char * input = text.c_str();
    while (input != nullptr) {
        int terminator = 0;
        const char * phonemes = espeak_TextToPhonemesWithTerminator((const void **)&input, espeakCHARS_UTF8, 2, &terminator);
        if (!phonemes) return 3;
        std::cout << terminator << '\t' << phonemes << '\n';
    }
    espeak_Terminate();
    return 0;
}
