import QtQuick
import QtQuick.Controls

Window {
    id: window
    width: 560
    height: 420
    minimumWidth: 260
    minimumHeight: 300
    visible: true
    title: "Park toolkit parent"
    color: "#31506f"
    flags: Qt.Window | Qt.FramelessWindowHint
    property int clicks: 0
    property int captureKeys: 0
    property var keyEvents: []

    function observeKey(event, pressed) {
        keyEvents.push([event.key, event.modifiers, pressed]);
        keyEvents = keyEvents.slice(-16);
        if (event.key === Qt.Key_F2 || event.key === Qt.Key_Control) {
            captureKeys++;
        }
        report("key");
        event.accepted = false;
    }

    function rect(item) {
        return [item.x, item.y, item.width, item.height];
    }

    function report(event) {
        console.log("PARK_TOOLKIT " + JSON.stringify({
            event: event,
            size: [width, height],
            text: field.text,
            dialogText: dialogField.text,
            dialogSize: [dialog.width, dialog.height],
            clicks: clicks,
            captureKeys: captureKeys,
            keyEvents: keyEvents,
            menuIndex: menu.currentIndex,
            nestedIndex: nested.currentIndex,
            controls: {
                field: rect(field),
                dialog: rect(openDialog),
                menu: rect(openMenu),
                count: rect(count),
                move: rect(move),
                resize: rect(resize)
            },
            dialogControls: {
                field: rect(dialogField),
                close: rect(closeDialog),
                deferred: rect(deferredClose)
            }
        }));
    }

    Component.onCompleted: Qt.callLater(report, "layout")
    onWidthChanged: Qt.callLater(report, "layout")
    onHeightChanged: Qt.callLater(report, "layout")

    Rectangle {
        id: move
        x: 0
        y: 0
        width: window.width
        height: 36
        color: "#253a51"
        Text {
            anchors.centerIn: parent
            text: "Move toolkit window"
            color: "white"
        }
        MouseArea {
            anchors.fill: parent
            onPressed: window.startSystemMove()
        }
    }

    TextField {
        id: field
        x: 16
        y: 56
        width: window.width - 32
        height: 36
        placeholderText: "Parent editor"
        onTextEdited: window.report("parent-edit")
        Keys.onPressed: event => window.observeKey(event, true)
        Keys.onReleased: event => window.observeKey(event, false)
    }

    Button {
        id: openDialog
        x: 16
        y: 108
        width: 200
        height: 36
        text: "Open owned dialog"
        onClicked: {
            dialog.show();
            dialog.requestActivate();
            Qt.callLater(dialogField.forceActiveFocus);
            window.report("dialog-request");
        }
    }

    Button {
        id: openMenu
        x: 16
        y: 160
        width: 200
        height: 36
        text: "Open popup menu"
        onClicked: menu.popup(x + width - 12, y + height)
    }

    Button {
        id: count
        x: 16
        y: 212
        width: 200
        height: 36
        text: "Ordinary clicks: " + window.clicks
        onClicked: {
            window.clicks++;
            window.report("count");
        }
    }

    Rectangle {
        id: resize
        x: window.width - 24
        y: window.height - 24
        width: 24
        height: 24
        color: "#bde4ed"
        MouseArea {
            anchors.fill: parent
            onPressed: window.startSystemResize(Qt.RightEdge | Qt.BottomEdge)
        }
    }

    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.RightButton
        onClicked: mouse => menu.popup(mouse.x, mouse.y)
    }

    Menu {
        id: menu
        width: 400
        popupType: Popup.Window
        onOpened: {
            currentIndex = -1;
            window.report("menu-open");
        }
        onCurrentIndexChanged: {
            if (visible)
                window.report("menu-selection");
        }
        onClosed: window.report("menu-close")
        background: Rectangle {
            color: "#d6a82f"
            border.color: "#282020"
            border.width: 2
        }
        MenuItem {
            text: "Confirm menu"
            onTriggered: window.report("menu-confirm")
        }
        Menu {
            id: nested
            title: "More"
            width: 240
            popupType: Popup.Window
            onOpened: {
                currentIndex = -1;
                window.report("nested-open");
            }
            onCurrentIndexChanged: {
                if (visible)
                    window.report("nested-selection");
            }
            background: Rectangle {
                color: "#659f88"
                border.color: "#282020"
                border.width: 2
            }
            MenuItem {
                text: "Nested action"
                onTriggered: window.report("nested-confirm")
            }
        }
    }

    Window {
        id: dialog
        title: "Park toolkit dialog"
        width: 320
        height: 180
        minimumWidth: 240
        minimumHeight: 150
        flags: Qt.Dialog | Qt.FramelessWindowHint
        modality: Qt.WindowModal
        color: "#6a3934"
        property bool closeOnDeactivate: false
        onWidthChanged: Qt.callLater(window.report, "dialog-layout")
        onHeightChanged: Qt.callLater(window.report, "dialog-layout")
        onActiveChanged: {
            if (!active && closeOnDeactivate) {
                closeOnDeactivate = false;
                deferredTimer.restart();
            }
        }
        Timer {
            id: deferredTimer
            interval: 8000
            onTriggered: {
                window.report("dialog-auto-close");
                dialog.close();
            }
        }
        onVisibleChanged: {
            if (visible)
                Qt.callLater(window.report, "dialog-visible");
        }
        TextField {
            id: dialogField
            x: 16
            y: 16
            width: dialog.width - 32
            height: 36
            placeholderText: "Dialog editor"
            onTextEdited: window.report("dialog-edit")
        }
        Button {
            id: closeDialog
            x: 16
            y: 72
            width: 200
            height: 36
            text: "Close owned dialog"
            onClicked: dialog.close()
        }
        Button {
            id: deferredClose
            x: 16
            y: 116
            width: 200
            height: 28
            text: "Close after losing focus"
            onPressed: window.report("dialog-deferred-pressed")
            onReleased: window.report("dialog-deferred-released")
            onClicked: {
                dialog.closeOnDeactivate = true;
                window.report("dialog-close-armed");
            }
        }
    }
}
