import QtQuick
import FuckMail.Icons

// Small line icons rendered as geometry, independent of installed symbol fonts.
// FmIconItem draws each name, colour and size once per window and shares that
// texture, so an icon costs one item instead of a Canvas or an Image.
FmIconItem {
    color: Theme.muted
}
