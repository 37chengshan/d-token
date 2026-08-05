#include <vector>

template <typename T>
T add(T left, T right) {
    return left + right;
}

struct Counter {
    int value = 0;

    int inc() {
        return ++value;
    }
};
