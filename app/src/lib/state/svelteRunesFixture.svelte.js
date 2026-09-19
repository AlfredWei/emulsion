class Counter {
  n = $state(0);
  double = $derived(this.n * 2);
  inc() { this.n += 1; }
}
export function createCounter() { return new Counter(); }

/** Mirrors the planned `install…()` shape: registers an effect, returns a stop function.
 * @param {{ double: number }} counter
 * @param {number[]} sink */
export function trackDouble(counter, sink) {
  return $effect.root(() => {
    $effect(() => {
      sink.push(counter.double);
    });
  });
}
