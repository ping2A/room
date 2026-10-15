/**
 * Game SFX + music playback worklet (main thread renders PCM; this plays it).
 * Message in:  { pcm: Float32Array } interleaved stereo at the AudioContext rate.
 * Message out: { buffered: frames, underruns: count } every ~10 ms. The main
 * thread renders on each report (audio clock), not only on rAF, so a slow or
 * hidden frame does not starve the device.
 */
class MixerPlaybackProcessor extends AudioWorkletProcessor {
  constructor(options) {
    super();
    const p = (options && options.processorOptions) || {};
    const sr = sampleRate || 44100;
    const bufSec = p.bufferSec || 0.45;
    this.cap = Math.max(2048, Math.round(sr * bufSec) * 2);
    this.buf = new Float32Array(this.cap);
    this.rd = 0;
    this.wr = 0;
    this.cnt = 0;
    this.sinceReport = 0;
    this.reportEvery = Math.max(128, Math.round(sr * 0.01));
    this.underruns = 0;
    this.started = false;
    this.port.onmessage = (e) => {
      const pcm = e.data && e.data.pcm;
      if (!pcm || !pcm.length) return;
      this.started = true;
      const n = pcm.length - (pcm.length % 2);
      if (n >= this.cap) {
        // Keep the tail if a huge chunk arrives.
        const start = n - (this.cap - (this.cap % 2));
        for (let i = start; i < n; i++) {
          this.buf[this.wr] = pcm[i];
          this.wr = (this.wr + 1) % this.cap;
        }
        this.cnt = Math.min(this.cap, n - start);
        this.rd = this.wr;
        return;
      }
      // Drop oldest if we would overflow (prefer low latency over backlog).
      while (this.cnt + n > this.cap) {
        this.rd = (this.rd + 2) % this.cap;
        this.cnt -= 2;
      }
      for (let i = 0; i < n; i++) {
        this.buf[this.wr] = pcm[i];
        this.wr = (this.wr + 1) % this.cap;
      }
      this.cnt += n;
    };
  }

  process(_inputs, outputs) {
    const outL = outputs[0] && outputs[0][0];
    const outR = outputs[0] && outputs[0][1];
    if (!outL) return true;
    const frames = outL.length;
    let starved = false;
    for (let i = 0; i < frames; i++) {
      if (this.cnt >= 2) {
        const l = this.buf[this.rd];
        this.rd = (this.rd + 1) % this.cap;
        const r = this.buf[this.rd];
        this.rd = (this.rd + 1) % this.cap;
        this.cnt -= 2;
        outL[i] = l;
        if (outR) outR[i] = r;
      } else {
        starved = true;
        outL[i] = 0;
        if (outR) outR[i] = 0;
      }
    }
    if (starved && this.started) this.underruns++;
    this.sinceReport += frames;
    // Report immediately on starvation so the main thread refills at once.
    if (starved || this.sinceReport >= this.reportEvery) {
      this.sinceReport = 0;
      this.port.postMessage({ buffered: this.cnt / 2, underruns: this.underruns });
    }
    return true;
  }
}

registerProcessor("room-mixer-playback", MixerPlaybackProcessor);
