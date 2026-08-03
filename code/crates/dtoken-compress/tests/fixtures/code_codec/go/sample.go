package codec

type Pair[T any] struct {
	Left  T
	Right T
}

func Add(left, right int) int {
	return left + right
}
