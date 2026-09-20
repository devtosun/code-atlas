package compatibility;

public record JavaBasic(int value) {
    public int doubled() {
        return value * 2;
    }
}
