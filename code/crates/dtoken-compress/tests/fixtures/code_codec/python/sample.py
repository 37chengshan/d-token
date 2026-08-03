from dataclasses import dataclass


@dataclass
class Point:
    x: int
    y: int


def add(left: int, right: int) -> int:
    return left + right
