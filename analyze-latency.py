"""Analyze mic_check --record-pair captures. Requires NumPy, keeps audio local."""
import json
import struct
import sys
from pathlib import Path
import numpy as np


def read(path):
    times, offsets, audio, flags = [], [], [], []
    total = 0
    with path.open('rb') as f:
        while header := f.read(16):
            qpc, count, flag = struct.unpack('<QII', header)
            samples = np.frombuffer(f.read(count * 4), dtype='<f4')
            if len(samples) != count or not np.isfinite(samples).all():
                raise ValueError('Invalid audio packet')
            flags.append(flag)
            if flag & 4 or (flag & 1 and total):
                raise ValueError('Unreliable timestamps or discontinuous recording; repeat capture')
            times.append(qpc / 10_000_000)
            offsets.append(total)
            total += count
            audio.append(samples)
    times, offsets = np.asarray(times), np.asarray(offsets)
    if np.any(np.diff(times) <= 0):
        raise ValueError('Non-monotonic capture timestamps')
    # Driver packet timestamps jitter. Fit each continuous endpoint's sample clock
    # against QPC, preserving its actual rate instead of overlapping packet samples.
    slope, intercept = np.polyfit(offsets, times-times[0], 1)
    residual = (times-times[0]) - (intercept+slope*offsets)
    if np.max(np.abs(residual)) > .005 or abs(slope*48000-1) > .005:
        raise ValueError('Capture clock is too irregular for this estimator')
    y = np.concatenate(audio)
    t = times[0]+intercept+slope*np.arange(total)
    return t, y, {'packets': len(flags), 'timestamp_errors': sum(bool(x & 4) for x in flags),
                  'discontinuities': sum(bool(x & 1) for x in flags),
                  'peak': float(np.max(np.abs(y))),
                  'fitted_sample_rate': float(1/slope),
                  'qpc_residual_p95_ms': float(np.percentile(np.abs(residual), 95)*1000),
                  'qpc_residual_max_ms': float(np.max(np.abs(residual))*1000)}


def lag_curve(raw, processed, max_lag, min_lag=0):
    # Pearson correlation of speech envelopes; this estimates delay, not voice quality.
    scores = []
    for lag in range(min_lag, max_lag + 1):
        a = raw[:len(raw)-lag] if lag>0 else raw[-lag:]
        b = processed[lag:] if lag>=0 else processed[:len(processed)+lag]
        a, b = a-a.mean(), b-b.mean()
        denom = np.linalg.norm(a) * np.linalg.norm(b)
        scores.append(float(np.dot(a, b) / denom) if denom > 1e-12 else 0.)
    return np.asarray(scores)


def waveform_lag(a, b, rate=16000, signed=False):
    a, b = a-a.mean(), b-b.mean()
    n = 1 << (2 * len(a) - 1).bit_length()
    corr = np.fft.irfft(np.conj(np.fft.rfft(a, n)) * np.fft.rfft(b, n), n)
    limit = rate // 2
    lags = np.arange(-limit if signed else 0, limit+1)
    score = np.abs(corr[lags % n]) / max(np.linalg.norm(a)*np.linalg.norm(b), 1e-12)
    peak = int(np.argmax(score))
    return {'lag_ms': int(lags[peak]) * 1000 / rate, 'correlation': float(score[peak])}


def main(folder, source='raw', destination='processed', report='analysis'):
    ta, a, da = read(folder/(source+'.pcmq'))
    tb, b, db = read(folder/(destination+'.pcmq'))
    start, end = max(ta[0], tb[0]), min(ta[-1], tb[-1])
    grid = start + np.arange(int((end-start)*16000)) / 16000
    a, b = np.interp(grid, ta, a), np.interp(grid, tb, b)
    count = len(a)//16*16
    ea = np.sqrt(np.mean(a[:count].reshape(-1, 16)**2, axis=1))
    eb = np.sqrt(np.mean(b[:count].reshape(-1, 16)**2, axis=1))
    ea, eb = [np.convolve(x, np.ones(10)/10, mode='valid') for x in (ea, eb)]
    min_lag = -500 if source=='loopback' else 0
    scores = lag_curve(ea, eb, 500, min_lag)
    peak = int(np.argmax(scores))
    segments = []
    for i in range(0, len(ea)-2500, 2000):
        score = lag_curve(ea[i:i+2500], eb[i:i+2500], 500, min_lag)
        p = int(np.argmax(score))
        segments.append({'start_s': i/1000, 'lag_ms': p+min_lag, 'correlation': float(score[p]),
                         'raw_rms': float(np.sqrt(np.mean(ea[i:i+2500]**2))),
                         'waveform': waveform_lag(a[i*16:(i+2500)*16], b[i*16:(i+2500)*16], signed=bool(min_lag))})
    result = {'duration_s': end-start, 'raw': da, 'processed': db,
              'envelope_lag_ms': peak+min_lag, 'envelope_correlation': float(scores[peak]),
              'waveform': waveform_lag(a, b, signed=bool(min_lag)), 'segments': segments,
              'scope': f'QPC-aligned {source} to {destination}; see endpoints.txt; excludes Discord and acoustic ADC delay'}
    (folder/(report+'.json')).write_text(json.dumps(result, indent=2), encoding='utf-8')
    print(json.dumps(result, indent=2))


def probe(folder):
    # Render submissions have scheduling jitter, so fit only the capture clock.
    # Match short nonperiodic blocks directly instead of inventing a uniform send clock.
    t, audio, metadata = read(folder/'processed.pcmq')
    capacity = int((folder/'probe.txt').read_text().split()[0].split('=')[1])
    results, last = [], float('-inf')
    with (folder/'raw.pcmq').open('rb') as f:
        while header := f.read(16):
            qpc, n, flags = struct.unpack('<QII', header)
            block = np.frombuffer(f.read(n*4), '<f4')
            if len(block) != n or not np.isfinite(block).all() or n>capacity:
                raise ValueError('Invalid probe packet')
            q = qpc/10_000_000
            if n<400 or np.std(block)<.001 or q-last<.75:
                continue
            last = q
            start, end = np.searchsorted(t, [q-.03, q+.5])
            samples = audio[start:end]
            if len(samples)<n:
                continue
            block = block-block.mean()
            corr = np.correlate(samples, block, 'valid')
            energy = np.convolve(samples*samples, np.ones(n), 'valid')
            score = corr/np.sqrt(np.maximum(energy*np.dot(block,block),1e-30))
            peak = int(np.argmax(score))
            delay = float((t[start+peak]-q)*1000)
            results.append({'submission_to_capture_ms':delay, 'queued_ms':(capacity-n)/48,
                            'less_queued_ms':delay-(capacity-n)/48, 'correlation':float(score[peak])})
    if len(results)<5 or min(x['correlation'] for x in results)<.9:
        raise ValueError('Insufficient matching probe blocks')
    report = {'method':'Individual synthetic blocks matched to CABLE Output. QPC at render submission; includes Windows/render queue. Queue subtraction is approximate.',
              'capture':metadata, 'blocks':results,
              'median_submission_ms':float(np.median([x['submission_to_capture_ms'] for x in results])),
              'median_less_queued_ms':float(np.median([x['less_queued_ms'] for x in results]))}
    (folder/'probe-analysis.json').write_text(json.dumps(report,indent=2),encoding='utf-8')
    print(json.dumps(report,indent=2))


if __name__ == '__main__':
    if sys.argv[1] == '--self-test':
        rng = np.random.default_rng(1)
        a = rng.random(4000)
        b = np.r_[np.zeros(83), a[:-83]]
        assert np.argmax(lag_curve(a, b, 500)) == 83
        assert waveform_lag(a, b)['lag_ms'] == 83/16
        assert np.argmax(lag_curve(b,a,500,-500))-500 == -83
        assert waveform_lag(b,a,signed=True)['lag_ms'] == -83/16
        print('PASS: known positive delay recovered')
    elif sys.argv[1] == '--routes':
        folder = Path(sys.argv[2])
        for source, destination, report in [('raw','processed','analysis'), ('raw','broadcast','broadcast'),
                                             ('raw','loopback','loopback-endpoint'), ('loopback','processed','loopback-to-output')]:
            main(folder, source, destination, report)
    elif sys.argv[1] == '--probe':
        probe(Path(sys.argv[2]))
    else:
        main(Path(sys.argv[1]))
