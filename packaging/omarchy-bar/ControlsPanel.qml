import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import qs.Ui as Ui
import qs.Commons as Commons

Ui.Panel {
  id: root
  moduleName: "io.github.muneer320.klicky-bar"
  manageIpc: false
  property var anchorItem: null
  property var hostWidget: null
  required property var engine
  property real volumeDraft: 0
  property bool volumeEditing: false

  function reveal(control) {
    if (!control.activeFocus)
      return;
    const point = control.mapToItem(layout, 0, 0);
    const viewport = scroll.contentItem as Flickable;
    if (!viewport)
      return;
    if (point.y < viewport.contentY)
      viewport.contentY = point.y;
    else if (point.y + control.height > viewport.contentY + scroll.availableHeight)
      viewport.contentY = point.y + control.height - scroll.availableHeight;
  }

  Connections {
    target: root.engine
    function onKnownChanged() {
      if (root.engine.known && root.volumeEditing && !volume.pressed)
        volumeCommit.restart();
    }
  }

  Ui.KeyboardPanel {
    id: popup
    anchorItem: root.anchorItem
    owner: root.hostWidget
    bar: root.bar
    open: root.opened
    focusTarget: panelContent
    contentWidth: popup.fittedContentWidth(Commons.Style.space(340))
    contentHeight: popup.fittedContentHeight(panelContent.implicitHeight)

    FocusScope {
      id: panelContent
      anchors.fill: parent
      implicitHeight: layout.implicitHeight
      Keys.onEscapePressed: root.close()
      ScrollView {
        id: scroll
        objectName: "klickyControlsScroll"
        anchors.fill: parent
        clip: true
        contentWidth: availableWidth
        contentHeight: layout.implicitHeight
        ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
        ColumnLayout {
          id: layout
          width: scroll.availableWidth
          spacing: Commons.Style.space(10)
          Label {
            text: "Klicky Bar"
            font.bold: true
            color: root.barForeground
          }
          Label {
            text: root.engine.busy ? "Updating…" : !root.engine.known ? "Engine unavailable" : !root.engine.running ? "Stopped" : root.engine.paused ? "Paused" : "Ready"
            color: root.barForeground
          }
          Label {
            Layout.fillWidth: true
            visible: text.length > 0
            text: root.engine.error || root.engine.actionError
            wrapMode: Text.WordWrap
            color: root.barForeground
          }
          RowLayout {
            Ui.Button {
              text: "Pause"
              focusable: true
              bordered: true
              opacity: enabled ? 1 : 0.4
              enabled: root.engine.known && root.engine.running && !root.engine.paused && !root.engine.busy
              onActiveFocusChanged: root.reveal(this)
              onClicked: root.engine.execute(["pause"])
            }
            Ui.Button {
              text: "Resume"
              focusable: true
              bordered: true
              opacity: enabled ? 1 : 0.4
              enabled: root.engine.known && root.engine.running && root.engine.paused && !root.engine.busy
              onActiveFocusChanged: root.reveal(this)
              onClicked: root.engine.execute(["resume"])
            }
          }
          Label {
            text: "Sound pack"
            color: root.barForeground
          }
          ComboBox {
            onActiveFocusChanged: root.reveal(this)
            Layout.fillWidth: true
            model: root.engine.packs
            currentIndex: root.engine.packs.indexOf(root.engine.soundPack)
            enabled: root.engine.known && !root.engine.busy
            onActivated: root.engine.execute(["switch", "--", currentText])
            Accessible.name: "Sound pack"
            palette.button: Commons.Color.popups.background
            palette.buttonText: root.barForeground
            palette.base: Commons.Color.popups.background
            palette.text: root.barForeground
            palette.highlight: Commons.Color.accent
          }
          Label {
            text: root.engine.known ? "Volume · " + Math.round(volume.value * 100) + "%" : "Volume"
            color: root.barForeground
          }
          Slider {
            id: volume
            objectName: "klickyVolume"
            onActiveFocusChanged: root.reveal(this)
            Layout.fillWidth: true
            from: 0
            to: 1
            stepSize: 0.01
            enabled: root.engine.known && !root.engine.busy
            Accessible.name: "Klicky volume"
            palette.highlight: Commons.Color.accent
            Binding {
              target: volume
              property: "value"
              value: root.engine.volume
              when: !volume.pressed && !root.volumeEditing && root.engine.known && !root.engine.busy
            }
            onMoved: {
              root.volumeEditing = true;
              root.volumeDraft = value;
              volumeCommit.restart();
            }
            onPressedChanged: if (!pressed && volumeCommit.running)
              volumeCommit.restart()
          }
          Timer {
            id: volumeCommit
            interval: 250
            onTriggered: {
              if (volume.pressed)
                restart();
              else if (root.engine.execute(["volume", root.volumeDraft.toFixed(2)])) {
                root.volumeEditing = false;
              }
            }
          }
          CheckBox {
            onActiveFocusChanged: root.reveal(this)
            text: "Start at login"
            palette.windowText: root.barForeground
            checked: root.engine.autostart
            enabled: root.engine.known && !root.engine.busy
            onClicked: root.engine.execute(["service", "autostart", checked ? "enable" : "disable"])
          }
          Label {
            text: "Audio: system default output"
            color: root.barForeground
          }
          RowLayout {
            Ui.Button {
              onActiveFocusChanged: root.reveal(this)
              focusable: true
              bordered: true
              opacity: enabled ? 1 : 0.4
              text: root.engine.running ? "Stop engine" : "Start engine"
              enabled: root.engine.known && !root.engine.busy
              onClicked: root.engine.execute(["service", root.engine.running ? "stop" : "start"])
            }
            Ui.Button {
              objectName: "klickyRefresh"
              text: "Refresh"
              focusable: true
              bordered: true
              enabled: !root.engine.busy
              onActiveFocusChanged: root.reveal(this)
              onClicked: root.engine.refresh()
            }
          }
        }
      }
    }
  }
}
