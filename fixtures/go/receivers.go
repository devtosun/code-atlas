package billing

type First struct{}
type Second struct{}

func (First) Render() string { return "first" }
func (Second) Render() string { return "second" }

func Shadow(value int) int {
	value := value + 1
	{
		value := value + 1
		return value
	}
}
