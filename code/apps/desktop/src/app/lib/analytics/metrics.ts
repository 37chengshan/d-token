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

export function aggregateTokenMetrics(rows: readonly TokenMetricInput[]) {
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

export function allocateIntegerTotal(totalValue: number, weightValues: number[]) {
  const total = Math.round(finiteNonNegative(totalValue));
  const weights = weightValues.map(finiteNonNegative);
  const weightTotal = weights.reduce((sum, weight) => sum + weight, 0);
  if (weightTotal === 0) return weights.map(() => 0);

  const raw = weights.map((weight) => (total * weight) / weightTotal);
  const allocated = raw.map(Math.floor);
  let remainder = total - allocated.reduce((sum, value) => sum + value, 0);
  const byLargestFraction = raw
    .map((value, index) => ({ index, fraction: value - Math.floor(value) }))
    .sort((a, b) => b.fraction - a.fraction || a.index - b.index);
  for (let index = 0; index < remainder; index += 1) {
    allocated[byLargestFraction[index].index] += 1;
  }
  return allocated;
}
