"""Extract named byte ranges from WARRIORS.WAD for inspection. Read-only.

    python research/scripts/dump_chunks.py --output research/reports/chunk-dump-1 \
        839135276:976 841255048:972 912332096:976 912625412:976

Each range is written as `<offset>-<size>.bin` next to a manifest recording the
source identity and each range's digest. Nothing is interpreted and nothing in
the archive is modified; this exists so a range the decoders refuse can be
looked at directly instead of guessed about.
"""
import argparse
import hashlib
import json
from pathlib import Path

from native_geometry_probe import ROOT
from load_map import SceneLoader


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--output', type=Path, required=True, help='new directory')
    parser.add_argument('ranges', nargs='+', metavar='OFFSET:SIZE')
    args = parser.parse_args()
    if args.output.exists():
        raise SystemExit('Output directory already exists; choose a new one.')
    wanted = []
    for item in args.ranges:
        offset, _, size = item.partition(':')
        offset, size = int(offset, 0), int(size, 0)
        if size <= 0 or size > 16 * 1024 * 1024:
            raise SystemExit('Range size outside the inspection budget: ' + item)
        wanted.append((offset, size))
    loader = SceneLoader()
    records = []
    try:
        total = (ROOT / 'WARRIORS.WAD').stat().st_size
        args.output.mkdir(parents=True)
        for offset, size in wanted:
            if offset < 0 or offset + size > total:
                raise SystemExit(f'Range outside the archive: {offset}:{size}')
            loader.wad.seek(offset)
            blob = loader.wad.read(size)
            if len(blob) != size:
                raise SystemExit(f'Short read at {offset}')
            name = f'{offset}-{size}.bin'
            (args.output / name).write_bytes(blob)
            records.append({'offset': offset, 'size': size, 'file': name,
                            'sha256': hashlib.sha256(blob).hexdigest()})
    finally:
        loader.close()
    manifest = {'parser': 'WarriorsChunkDump/0.1', 'source_file': 'WARRIORS.WAD',
                'source_identities': loader.identities, 'ranges': records,
                'scope': 'Raw byte extraction for inspection; nothing is interpreted.'}
    with (args.output / 'manifest.json').open('w', encoding='utf-8') as handle:
        json.dump(manifest, handle, indent=2)
    print(json.dumps({'ranges': len(records), 'output': str(args.output)}))


if __name__ == '__main__':
    main()
