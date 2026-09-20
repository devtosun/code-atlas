package compatibility

type GoBasic struct {
	Value int
}

func goBasic(value int) int {
	item := GoBasic{Value: value}
	return item.Value * 2
}
