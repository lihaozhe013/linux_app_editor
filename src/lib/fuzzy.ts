/**
 * Small deterministic fuzzy matcher: substring matches rank highest
 * (word-start matches higher still), otherwise a subsequence match with
 * consecutive-run and word-start bonuses. Returns null when the needle is
 * not a subsequence of the haystack.
 */
const WORD_BOUNDARY = /[\s/._@-]/;

export function fuzzyScore(needle: string, haystack: string): number | null {
  if (needle.length === 0) {
    return 0;
  }
  const n = needle.toLowerCase();
  const h = haystack.toLowerCase();
  if (h.length === 0) {
    return null;
  }

  const direct = h.indexOf(n);
  if (direct >= 0) {
    let score = 120 - Math.min(direct, 40);
    if (direct === 0 || WORD_BOUNDARY.test(h[direct - 1] ?? '')) {
      score += 25;
    }
    score -= Math.min(h.length, 200) * 0.05;
    return Math.round(score);
  }

  let cursor = 0;
  let score = 0;
  let streak = 0;
  let previous = -2;
  for (const ch of n) {
    const found = h.indexOf(ch, cursor);
    if (found < 0) {
      return null;
    }
    score += 8 - Math.min(found - cursor, 8);
    if (found === previous + 1) {
      streak += 1;
      score += 6 * streak;
    } else {
      streak = 0;
    }
    if (found === 0 || WORD_BOUNDARY.test(h[found - 1] ?? '')) {
      score += 10;
    }
    previous = found;
    cursor = found + 1;
  }
  score -= Math.min(h.length, 300) * 0.05;
  return Math.round(score);
}

/** Best score across fields; earlier fields win ties. */
export function fuzzyScoreFields(needle: string, fields: string[]): number | null {
  let best: number | null = null;
  for (const [index, field] of fields.entries()) {
    const score = fuzzyScore(needle, field);
    if (score !== null) {
      const weighted = score - index * 2;
      if (best === null || weighted > best) {
        best = weighted;
      }
    }
  }
  return best;
}
