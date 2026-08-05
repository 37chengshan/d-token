function add(left, right) {
  return left + right;
}

class Counter {
  #count = 0;

  inc() {
    this.#count += 1;
    return this.#count;
  }
}
