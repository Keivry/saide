package com.genymobile.scrcpy.control;

// PATCH: anti-detection - new device monitoring message types
public static final int TYPE_ROTATION_CHANGED = 3;
public static final int TYPE_IME_STATE_CHANGED = 4;

public final class DeviceMessage {

    public static final int TYPE_CLIPBOARD = 0;
    public static final int TYPE_ACK_CLIPBOARD = 1;
    public static final int TYPE_UHID_OUTPUT = 2;

    private int type;
    private String text;
    private long sequence;
    private int id;
    private byte[] data;
    // PATCH: anti-detection - fields for device monitoring messages
    private int rotation;
    private boolean imeVisible;

    private DeviceMessage() {
    }

    public static DeviceMessage createClipboard(String text) {
        DeviceMessage event = new DeviceMessage();
        event.type = TYPE_CLIPBOARD;
        event.text = text;
        return event;
    }

    public static DeviceMessage createAckClipboard(long sequence) {
        DeviceMessage event = new DeviceMessage();
        event.type = TYPE_ACK_CLIPBOARD;
        event.sequence = sequence;
        return event;
    }

    public static DeviceMessage createUhidOutput(int id, byte[] data) {
        DeviceMessage event = new DeviceMessage();
        event.type = TYPE_UHID_OUTPUT;
        event.id = id;
        event.data = data;
        return event;
    }

    // PATCH: anti-detection - factory for rotation and IME state changed
    public static DeviceMessage createRotationChanged(int rotation) {
        DeviceMessage msg = new DeviceMessage();
        msg.type = TYPE_ROTATION_CHANGED;
        msg.rotation = rotation;
        return msg;
    }

    public static DeviceMessage createImeStateChanged(boolean visible) {
        DeviceMessage msg = new DeviceMessage();
        msg.type = TYPE_IME_STATE_CHANGED;
        msg.imeVisible = visible;
        return msg;
    }

    public int getType() {
        return type;
    }

    public String getText() {
        return text;
    }

    public long getSequence() {
        return sequence;
    }

    public int getId() {
        return id;
    }

    public byte[] getData() {
        return data;
    }

    // PATCH: anti-detection - getters for device monitoring
    public int getRotation() {
        return rotation;
    }

    public boolean getImeVisible() {
        return imeVisible;
    }
}
