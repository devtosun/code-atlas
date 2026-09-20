namespace Compatibility;

public sealed class CSharpBasic
{
    public CSharpBasic(int value) => Value = value;
    public int Value { get; }
    public async Task<int> DoubleAsync() => await Task.FromResult(Value * 2);
}
