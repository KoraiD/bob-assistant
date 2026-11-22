# Voice Dictation: requirements

Required capability. Hold a hotkey, speak, release; text lands at the caret. EARS IDs `VD-n`.

## User stories
- As a user, I want to dictate into any app with a single held key.
- As a privacy-conscious user, I want transcription to run fully offline.

## Requirements
- **VD-1** WHILE the dictation hotkey (default Option+Space) is held, THE System SHALL capture microphone audio at 16 kHz mono.
- **VD-2** WHEN the hotkey is released, THE System SHALL transcribe the audio locally and insert the text at the caret.
- **VD-3** THE System SHALL show the orb in a listening state while recording and a thinking state while transcribing.
- **VD-4** IF the hold is shorter than 250 ms, THEN THE System SHALL discard the audio and insert nothing.
- **VD-5** IF no speech is detected (VAD), THEN THE System SHALL insert nothing and say so.
- **VD-6** WHERE a cleanup preset (Clean, Translate to English, Meeting notes) is enabled, THE System SHALL pass the transcript through the dictation brain, and SHALL fall back to the raw transcript on failure.
- **VD-7** WHEN Microphone permission is denied, THE System SHALL explain how to grant it and SHALL NOT crash.
- **VD-8** THE System SHALL NOT write audio to disk and SHALL NOT log transcripts.
- **VD-9** WHILE the focused element is a secure text field, THE System SHALL NOT insert text.
- **VD-10** WITH a short utterance on an M3/16 GB, THE System SHALL insert text within 1.5 s of release (target; measured).
- **VD-11** THE System SHALL cap one recording at 5 minutes.

## Out of scope
Streaming partial transcripts, speaker diarization, wake word.
