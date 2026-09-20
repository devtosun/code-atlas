class DartBasic {
  const DartBasic(this.value);
  final int value;
  int doubled() => value * 2;
}

void main() {
  const item = DartBasic(21);
  print(item.doubled());
}
