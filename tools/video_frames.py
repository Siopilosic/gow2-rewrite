"""Cut a gameplay recording into numbered frames and a contact sheet, to compare with the viewer's clip frames.

  python tools/video_frames.py reference/square1.mp4 --every 3 --out reference/square1
  python tools/video_frames.py reference/square1.mp4 --start 2.0 --end 4.5 --every 2 --cols 6

Writes <out>/frame_<index>.png for every selected frame and <out>/sheet.png (a contact sheet, frame numbers and
times stamped on each cell). `--every N` keeps every N-th video frame. The recording is read as is: for a 60 fps
capture of the PS2 game, 2 video frames are about one 30 fps animation frame (the game's clips use dt = 1/30).
"""
import argparse
import os

import cv2


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("video")
    ap.add_argument("--out", default=None)
    ap.add_argument("--every", type=int, default=3)
    ap.add_argument("--start", type=float, default=0.0, help="seconds")
    ap.add_argument("--end", type=float, default=None, help="seconds")
    ap.add_argument("--cols", type=int, default=5)
    ap.add_argument("--cell", type=int, default=380, help="contact sheet cell width")
    ap.add_argument("--max", type=int, default=30, help="most frames in the sheet")
    a = ap.parse_args()
    cap = cv2.VideoCapture(a.video)
    if not cap.isOpened():
        raise SystemExit(f"cannot open {a.video}")
    fps = cap.get(cv2.CAP_PROP_FPS) or 30.0
    total = int(cap.get(cv2.CAP_PROP_FRAME_COUNT))
    w, h = int(cap.get(cv2.CAP_PROP_FRAME_WIDTH)), int(cap.get(cv2.CAP_PROP_FRAME_HEIGHT))
    print(f"{a.video}: {w}x{h}, {fps:.2f} fps, {total} frames, {total / fps:.2f} s")
    out = a.out or os.path.splitext(a.video)[0]
    os.makedirs(out, exist_ok=True)
    first = int(a.start * fps)
    last = total if a.end is None else min(total, int(a.end * fps))
    picked = []
    for idx in range(first, last, a.every):
        cap.set(cv2.CAP_PROP_POS_FRAMES, idx)
        ok, img = cap.read()
        if not ok:
            break
        cv2.imwrite(os.path.join(out, f"frame_{idx:05d}.png"), img)
        picked.append((idx, img))
    print(f"wrote {len(picked)} frames to {out}")
    shown = picked[: a.max]
    if shown:
        cw = a.cell
        chh = int(cw * h / w)
        rows = (len(shown) + a.cols - 1) // a.cols
        sheet = cv2.UMat(rows * chh, a.cols * cw, cv2.CV_8UC3).get() * 0
        for k, (idx, img) in enumerate(shown):
            small = cv2.resize(img, (cw, chh))
            cv2.putText(small, f"f{idx} t={idx / fps:.2f}s", (6, 18), cv2.FONT_HERSHEY_SIMPLEX, 0.55, (0, 255, 255), 1, cv2.LINE_AA)
            r, c = divmod(k, a.cols)
            sheet[r * chh:(r + 1) * chh, c * cw:(c + 1) * cw] = small
        cv2.imwrite(os.path.join(out, "sheet.png"), sheet)
        print("contact sheet", os.path.join(out, "sheet.png"))


if __name__ == "__main__":
    main()