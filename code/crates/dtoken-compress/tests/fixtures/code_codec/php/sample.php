<?php

namespace Acme\Codec;

use Acme\Support\Result;

class SampleService
{
    private Result $result;

    public function __construct(Result $result)
    {
        $this->result = $result;
    }

    public function render(array $rows): string
    {
        $parts = [];
        foreach ($rows as $row) {
            $parts[] = $this->result->format($row);
        }
        return implode(PHP_EOL, $parts);
    }
}

function helper(int $value): int
{
    return $value * 2;
}
