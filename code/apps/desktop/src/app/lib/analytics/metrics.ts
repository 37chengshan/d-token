export type TokenMetricInput = { before: number; after: number };

export type TokenMetrics = TokenMetricInput & {
  saved: number;
  savingsPercent: number;
};

function finiteNonNegative(value: number) {
  return Number.isFinite(value) ? Math.max(0, value) : 0;
}

export function deriveTokenMetrics(beforeValue: number, afterValue: number): TokenMetrics {
  const before = finiteNonNegative(beforeValue);
  const after = Math.min(before, finiteNonNegative(afterValue));
  const saved = before - after;
  return {
    before,
    after,
    saved,
    savingsPercent: before > 0 ? Number(((saved / before) * 100).toFixed(1)) : 0,
  };
}

export function aggregateTokenMetrics(rows: TokenMetricInput[]) {
  const totals = rows.reduce(
    (sum, row) => ({ before: sum.before + row.before, after: sum.after + row.after }),
    { before: 0, after: 0 },
  );
  return deriveTokenMetrics(totals.before, totals.after);
}

export function deriveLatencyTotal(parts: Record<string, number>) {
  return Number(
    Object.values(parts)
      .reduce((total, part) => total + finiteNonNegative(part), 0)
      .toFixed(1),
  );
}
