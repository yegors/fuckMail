// SpellChecker::platform() where the platform offers no spell checker to Qt
// programs (Linux): there is none, so the composer shows no spelling marks.
#include "SpellChecker.h"

SpellChecker *SpellChecker::platform() { return nullptr; }
