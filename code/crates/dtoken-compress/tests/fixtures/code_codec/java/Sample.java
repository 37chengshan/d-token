package codec;

class Pair {
    private int left;
    private int right;
    Pair(int left, int right) {
        this.left = left;
        this.right = right;
    }
}

class Sample {
    static int add(int left, int right) {
        return left + right;
    }
}
