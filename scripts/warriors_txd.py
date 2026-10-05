"""PS2 texture-dictionary reading: structural walk, raster classification, bounded decode.

This module generalises the structure of the verified single-material slice
(`texture_slice.py`) without generalising its *claims*. Three separate layers:

  1. `dictionary_textures` / `texture_chunks` - chunk structure only. Backed by
     SLUS_212.15 0x4810f8 (dictionary count/device, 0x15 entries) and 0x4954f0
     (PS2 FourCC, name/mask strings, raster reader call).
  2. `raster_header` - field extraction from the 64-byte raster struct, backed by
     0x4950d8. This reads fields; it asserts no meaning for them.
  3. `PROFILES` - the only place where a raster is claimed to be *decodable*.
     A profile must be registered with an evidence note before its pixels are
     produced. Unregistered field combinations classify as UNSUPPORTED and are
     reported with their measured fields, never guessed.

The transfer walk is a loop rather than a single-packet assertion. That is a
structural relaxation (the GIF tag checks are unchanged), so mip chains and
multi-packet uploads can be *counted and located* before any claim is made about
how the game uses them.

Reader facts established from the 0x4950d8 disassembly in
`research/reports/material-slice-verified/evidence-004950d8.asm.txt`, and used
to shape classification here:

  * The version halfword at +0x0e selects the layout. `sltiu v0, version, 2` at
    0x495150 and 0x49520c splits versions 0 and 1 onto a legacy path that reads
    the payload as one linear blob and derives the palette pointer as
    `pixels + pixel_size`. At 0x495314 the remaining path does `bne a0, 2` and
    fails, so the transfer-packet layout this module reads is version 2 exactly.
  * Level count is `(tex1 >> 2) + 1`: 0x495170 does `srl v0, v0, 2` on the word
    at +0x1c and 0x495178 adds one before passing it to the raster allocator at
    0x486120 with width, height, depth and the format halfword. Stored MXL is
    therefore the mip count, and a raster with MXL n must carry n+1 pixel
    transfers. That consistency is now measured per raster.
  * `palette_size` zero is legitimate, not malformed: 0x495348 does
    `beqz a2, 0x495378` and skips the CLUT read entirely. Unpaletted rasters
    exist and this reader handles them, so they are classified as their own
    family rather than reported as a missing palette.
  * The CLUT read at 0x495358 targets `palette_pointer - 0x50`, which is the
    80-byte GIF A+D plus IMAGE header this module parses. That independently
    confirms the packet header size.

Corpus measurement (`research/reports/texture-survey-1`, 22,791 rasters across
2,047 dictionaries) settled three things that the single-slice work could not:

  * Every raster in the archive is version 2 and paletted, PSMT4 or PSMT8 with
    CSM1 and CPSM 0. There is no version 0/1 population and no unpaletted
    population, so those reader paths, while real, have nothing to decode here.
  * Format word `0x8000` is the mipmap flag. It is set on exactly the 703
    rasters with MXL >= 1 and on no other, and those rasters carry exactly
    MXL + 1 pixel transfers, with zero disagreements corpus-wide.
  * The upload shape is `transfer_width = max(level_width / 2, minimum)` with
    the minimum being one block of the upload format: 16 for PSMT4 uploaded as
    PSMCT16, 8 for PSMT8 uploaded as PSMCT32. This predicted all 22,787 level-0
    shapes and all 2,605 mip-level shapes with no misses.

The last point fixes a latent error. The unswizzle stride is the *upload buffer*
width, `2 * transfer_width`, not the texture width. They are equal whenever the
texture is wide enough to need no padding, which is why the verified slice was
unaffected, but for a narrow texture they differ and the texture-width reading
maps two texels onto one byte. Checked over the 33 distinct shapes present in
the archive, the buffer-width reading is collision-free on all of them and the
texture-width reading collides on five.

Nothing here converts colour spaces, applies alpha policy or orders draws.
"""
import struct

from native_geometry_probe import Unsupported, take, words, span
from inspect_world_geometry import children

STAMP = 0x1c02000a
CHUNK_STRUCT, CHUNK_STRING, CHUNK_EXTENSION = 1, 2, 3
CHUNK_TEXTURE_NATIVE, CHUNK_TEXTURE_DICTIONARY = 0x15, 0x16
PS2_FOURCC = 0x00325350

# RenderWare raster format flags. 0x8000 is the mipmap bit, confirmed by its
# exact correlation with a non-zero stored MXL across the whole archive.
FORMAT_MIPMAP = 0x8000
BASE_FORMAT_WORD = {4: 0x4504, 8: 0x2504}
# Indexed data is uploaded through a colour format: PSMT4 as PSMCT16, PSMT8 as
# PSMCT32. The minimum transfer width is one block of that format.
UPLOAD_MINIMUM_WIDTH = {4: 16, 8: 8}
UPLOAD_BYTES_PER_TEXEL = {4: 2, 8: 4}
PALETTE_SHAPE = {4: (8, 3, 96), 8: (16, 16, 1024)}


def level_shape(width, height, depth, level):
    """Texture and upload dimensions for one mip level.

    Returns (level_width, level_height, transfer_width, transfer_height,
    transfer_bytes, buffer_width). `buffer_width` is the unswizzle stride in
    texels and is what `index_address` must be given.
    """
    w = max(width >> level, 1)
    h = max(height >> level, 1)
    transfer_width = max(w // 2, UPLOAD_MINIMUM_WIDTH[depth])
    transfer_height = max(h // 2, 1)
    payload = transfer_width * transfer_height * UPLOAD_BYTES_PER_TEXEL[depth]
    return w, h, transfer_width, transfer_height, payload, 2 * transfer_width

# GS pixel storage modes, PS2 GS manual Table 8-1. Names only; no decoder is
# implied by presence in this table.
PSM_NAMES = {0: 'PSMCT32', 1: 'PSMCT24', 2: 'PSMCT16', 10: 'PSMCT16S', 19: 'PSMT8',
             20: 'PSMT4', 27: 'PSMT8H', 36: 'PSMT4HL', 44: 'PSMT4HH',
             48: 'PSMZ32', 49: 'PSMZ24', 50: 'PSMZ16', 58: 'PSMZ16S'}


def psm_name(value):
    return PSM_NAMES.get(value, f'PSM_{value}')


def read_string(data, chunk):
    """Type-2 string chunk. Padding must be zero; the original writer pads with zero."""
    kind, offset, length = chunk
    if kind != CHUNK_STRING:
        raise Unsupported('not a string chunk')
    raw = take(data, offset + 12, length)
    if not 0 < length <= 512 or b'\0' not in raw:
        raise Unsupported('invalid texture string')
    value, padding = raw.split(b'\0', 1)
    if any(padding):
        raise Unsupported('nonzero string padding')
    try:
        return value.decode('ascii')
    except UnicodeDecodeError:
        raise Unsupported('non-ascii texture string')


def single(chunks, kind):
    found = [c for c in chunks if c[0] == kind]
    if len(found) != 1:
        raise Unsupported('missing/duplicate chunk ' + hex(kind))
    return found[0]


def texture_chunks(dictionary, base=0):
    """Walk a type-0x16 dictionary chunk (header included).

    Evidence: 0x4810f8 reads a count/device pair from the dictionary struct and
    then consumes type-0x15 children. The count/device agreement is reported,
    not enforced, so a disagreeing dictionary can be measured rather than lost.
    """
    kind, length, stamp = words(dictionary, 0, 3)
    if kind != CHUNK_TEXTURE_DICTIONARY or stamp != STAMP:
        raise Unsupported('not a texture dictionary chunk')
    if length + 12 != len(dictionary):
        raise Unsupported('texture dictionary envelope')
    entries = children(dictionary, 12, len(dictionary))
    header = single(entries, CHUNK_STRUCT)
    if header[2] < 4:
        raise Unsupported('short dictionary struct')
    count, device = struct.unpack('<HH', take(dictionary, header[1] + 12, 4))
    native = [c for c in entries if c[0] == CHUNK_TEXTURE_NATIVE]
    other = [c for c in entries if c[0] not in (CHUNK_STRUCT, CHUNK_TEXTURE_NATIVE)]
    return {'source': span(base, len(dictionary)), 'declared_count': count,
            'device': device, 'native_count': len(native),
            'count_matches': count == len(native), 'device_is_ps2': device == 6,
            'other_chunk_types': sorted({c[0] for c in other}),
            'textures': [{'chunk': c, 'source': span(base + c[1], c[2] + 12)} for c in native]}


def raster_header(data, offset):
    """The 64-byte raster structure read by 0x4950d8. Field extraction only."""
    (width, height, depth, format_word, version, tex0, palette_offset, tex1,
     miptbp1, miptbp2, pixel_size, palette_size, allocation,
     tail) = struct.unpack('<IIIHHQIIQQIIII', take(data, offset, 64))
    return {
        'width': width, 'height': height, 'depth': depth,
        'format_word': format_word, 'version': version,
        'tex0': tex0, 'tex1': tex1, 'palette_offset': palette_offset,
        'miptbp1': miptbp1, 'miptbp2': miptbp2,
        'pixel_size': pixel_size, 'palette_size': palette_size,
        'allocation_size': allocation, 'header_tail_raw': tail,
        # GS TEX0/TEX1 bit fields, PS2 GS manual. Decoded for reporting.
        'tbp0': tex0 & 0x3fff, 'tbw': (tex0 >> 14) & 63,
        'psm': (tex0 >> 20) & 63, 'tw': (tex0 >> 26) & 15, 'th': (tex0 >> 30) & 15,
        'tcc': (tex0 >> 34) & 1, 'tfx': (tex0 >> 35) & 3,
        'cbp': (tex0 >> 37) & 0x3fff, 'cpsm': (tex0 >> 51) & 15,
        'csm': (tex0 >> 55) & 1, 'csa': (tex0 >> 56) & 31, 'cld': (tex0 >> 61) & 7,
        'tex1_mxl': (tex1 >> 2) & 7,
        # 0x495170..0x495178: (tex1 >> 2) + 1 is passed to the raster allocator
        # as the level count, so MXL n implies n+1 stored pixel transfers.
        'expected_levels': ((tex1 >> 2) & 7) + 1,
        'tex1_mmag': (tex1 >> 5) & 1,
        'tex1_mmin': (tex1 >> 6) & 7, 'tex1_mtba': (tex1 >> 9) & 1,
        'header_source': span(offset, 64), 'header_hex': take(data, offset, 64).hex()}


def transfers(payload, base):
    """Sequence of GIF A+D (TRXPOS/TRXREG/TRXDIR) + IMAGE uploads inside one region.

    The per-packet checks are those already validated for the reference material.
    The only relaxation is that the region may hold more than one packet, so mip
    levels and split uploads are located instead of rejected outright.
    """
    result, cursor = [], 0
    while cursor < len(payload):
        if len(payload) - cursor < 80:
            raise Unsupported('truncated transfer packet')
        if words(payload, cursor, 4) != (3, 0x10000000, 14, 0):
            raise Unsupported('unsupported GIF register packet')
        registers = {}
        for at, expected in ((16, 0x51), (32, 0x52), (48, 0x53)):
            value, register = struct.unpack('<QQ', take(payload, cursor + at, 16))
            if register != expected:
                raise Unsupported('unexpected transfer register')
            registers[register] = value
        if registers[0x53] != 0:
            raise Unsupported('transfer is not host to local')
        tag = words(payload, cursor + 64, 4)
        if tag[0] > 0x7fff or tag[1:] != (0x08000000, 0, 0):
            raise Unsupported('unsupported GIF IMAGE packet')
        length = tag[0] * 16
        take(payload, cursor + 80, length)
        trxpos, trxreg = registers[0x51], registers[0x52]
        result.append({
            'trxpos': trxpos, 'trxreg': trxreg,
            'dsax': (trxpos >> 32) & 0x7ff, 'dsay': (trxpos >> 48) & 0x7ff,
            'transfer_width': trxreg & 0xfff, 'transfer_height': (trxreg >> 32) & 0xfff,
            'source': span(base + cursor, 80 + length),
            'image': span(base + cursor + 80, length)})
        cursor += 80 + length
    if not result:
        raise Unsupported('empty transfer region')
    return result


def texture_fields(data, base=0):
    """Structure + raster fields + transfer map for one 0x15 texture chunk.

    `data` is the texture chunk with its own 12-byte header at index 0.
    Raises Unsupported only for structural violations, never for an unfamiliar
    raster profile - classification is the caller's job via `classify`.
    """
    kind, length, stamp = words(data, 0, 3)
    if kind != CHUNK_TEXTURE_NATIVE or stamp != STAMP or length + 12 != len(data):
        raise Unsupported('native texture envelope')
    entries = children(data, 12, len(data))
    profile = [c[0] for c in entries]
    if profile[:4] != [CHUNK_STRUCT, CHUNK_STRING, CHUNK_STRING, CHUNK_STRUCT]:
        raise Unsupported(f'native texture child profile {profile}')
    if entries[0][2] < 8 or words(data, entries[0][1] + 12, 1)[0] != PS2_FOURCC:
        raise Unsupported('not a PS2 native texture')
    addressing = words(data, entries[0][1] + 16, 1)[0]
    name = read_string(data, entries[1])
    mask = read_string(data, entries[2])
    _, raster_at, raster_len = entries[3]
    raster = children(data, raster_at + 12, raster_at + 12 + raster_len)
    if [c[0] for c in raster] != [CHUNK_STRUCT, CHUNK_STRUCT] or raster[0][2] != 64:
        raise Unsupported('native raster structure')
    header = raster_header(data, raster[0][1] + 12)
    header['header_source'] = span(base + raster[0][1] + 12, 64)
    _, payload_at, payload_len = raster[1]
    if payload_len != header['pixel_size'] + header['palette_size']:
        raise Unsupported('native raster payload length mismatch')
    payload = take(data, payload_at + 12, payload_len)
    pixel_region = take(payload, 0, header['pixel_size'])
    palette_region = take(payload, header['pixel_size'], header['palette_size'])
    record = {'name': name, 'mask': mask, 'chunk_types': profile,
              'filter_addressing': addressing,
              'source': span(base, len(data)),
              'payload_source': span(base + payload_at + 12, payload_len),
              **header}
    record['pixel_transfers'] = transfers(pixel_region, base + payload_at + 12)
    record['palette_transfers'] = (
        transfers(palette_region, base + payload_at + 12 + header['pixel_size'])
        if header['palette_size'] else [])
    record['pixel_region'] = pixel_region
    record['palette_region'] = palette_region
    record['paletted'] = bool(header['palette_size'])
    record['levels_consistent'] = (
        len(record['pixel_transfers']) == header['expected_levels'])
    return record


def profile_key(record):
    """Compact identity used for corpus histograms and decoder registration."""
    palette = 'pal' if record.get('paletted', bool(record['palette_size'])) else 'nopal'
    return (f"v{record['version']}/{psm_name(record['psm'])}/d{record['depth']}"
            f"/{palette}/cpsm{record['cpsm']}/csm{record['csm']}"
            f"/levels{len(record['pixel_transfers'])}"
            f"/mxl{record['tex1_mxl']}")


# --------------------------------------------------------------------------
# Decoder registry. A profile appears here only with an evidence note.
# --------------------------------------------------------------------------

def _gates_indexed_csm1(record):
    """Conditions for the indexed PS2 path, widened by the corpus survey."""
    gates = []
    w, h, depth = record['width'], record['height'], record['depth']
    if record['version'] != 2:
        gates.append('version')
    if depth not in (4, 8):
        gates.append('depth')
        return gates
    levels = len(record['pixel_transfers'])
    expected_format = BASE_FORMAT_WORD[depth] | (FORMAT_MIPMAP if levels > 1 else 0)
    if record['format_word'] != expected_format:
        gates.append('format_word')
    if record['psm'] != (20 if depth == 4 else 19):
        gates.append('psm_vs_depth')
    if not (1 <= w <= 1024 and 1 <= h <= 1024 and w & (w - 1) == 0 and h & (h - 1) == 0):
        gates.append('dimensions')
    elif (record['tw'], record['th']) != (w.bit_length() - 1, h.bit_length() - 1):
        gates.append('tex0_dimensions')
    if record['cpsm'] != 0 or record['csm'] != 0 or record['csa'] != 0:
        gates.append('clut_mode')
    if not record.get('levels_consistent', True):
        # MXL and the stored transfer count disagree. Reported, never repaired.
        gates.append('level_count_disagrees_with_mxl')
    if not record.get('paletted', bool(record['palette_size'])):
        gates.append('unpaletted')
    if levels > 8 or levels < 1:
        gates.append('pixel_transfer_count')
    if len(record['palette_transfers']) != 1:
        gates.append('palette_transfer_count')
    if gates:
        return gates
    # Every level's upload must match the measured shape rule exactly.
    for level, transfer in enumerate(record['pixel_transfers']):
        _, _, tw, th, payload, _ = level_shape(w, h, depth, level)
        if (transfer['transfer_width'], transfer['transfer_height']) != (tw, th):
            gates.append(f'pixel_transfer_shape_level{level}')
        if transfer['image']['size'] != payload:
            gates.append(f'packed_texel_size_level{level}')
    pixel, palette = record['pixel_transfers'][0], record['palette_transfers'][0]
    # Only the level-0 pixel destination is constrained. Mip levels land at
    # non-zero offsets inside the same allocation, and the CLUT destination is a
    # runtime GS buffer offset chosen by the uploader (BITBLTBUF is set outside
    # this packet). Both are recorded, not gated.
    if pixel['dsax'] or pixel['dsay']:
        gates.append('pixel_transfer_destination')
    palette_width, palette_height, palette_bytes = PALETTE_SHAPE[depth]
    if (palette['transfer_width'], palette['transfer_height']) != (palette_width,
                                                                   palette_height):
        gates.append('palette_transfer_shape')
    if palette['image']['size'] != palette_bytes:
        gates.append('palette_footprint')
    return gates


def index_address(x, y, width):
    """PS2 RW upload permutation, expressed as bit routing within four rows.

    Reference: aap/librw src/ps2/ps2raster.cpp swizzle/unswizzleRaster, checked
    independently against PCSX2 GS page/block/column addressing for 32, 64 and
    128 pixel footprints. Address units are bytes for PAL8, nibbles for PAL4.

    `width` is the upload buffer width in texels, which equals twice the GIF
    transfer width. For a texture wide enough to need no padding that is the
    texture width, which is the case the reference slice verified; for a narrow
    texture it is wider, and using the texture width instead makes this mapping
    collide.
    """
    x2 = ((x >> 2) ^ (y >> 1) ^ (y >> 2)) & 1
    lane = ((y >> 1) & 1) | (((x >> 3) & 1) << 1)
    column = (x & 3) | (x2 << 2) | ((x >> 4) << 3)
    return (y // 4) * width * 4 + (y & 1) * width * 2 + column * 4 + lane


def clut_index(index, depth):
    """256-entry CSM1 CLUT byte order. 16-entry PAL4 CLUTs are not reordered."""
    if depth == 4:
        return index
    return (index & ~24) | ((index & 8) << 1) | ((index & 16) >> 1)


def _decode_indexed_csm1(record):
    """Every stored level as raw GS RGBA, plus per-texel source offsets.

    Level 0 is what exporters and the viewer consume; the rest are carried so a
    caller that wants the stored chain has it without a second decode. The
    unswizzle stride for each level is that level's upload buffer width, which
    is twice its GIF transfer width.
    """
    depth = record['depth']
    palette_image = record['palette_transfers'][0]['image']
    palette_region_base = record['payload_source']['offset'] + record['pixel_size']
    palette = take(record['palette_region'],
                   palette_image['offset'] - palette_region_base,
                   palette_image['size'])
    palette_base = palette_image['offset']
    pixel_region_base = record['payload_source']['offset']
    images = []
    for level, transfer in enumerate(record['pixel_transfers']):
        w, h, _, _, _, buffer_width = level_shape(
            record['width'], record['height'], depth, level)
        image = transfer['image']
        packed = take(record['pixel_region'], image['offset'] - pixel_region_base,
                      image['size'])
        rgba = bytearray()
        indices = bytearray()
        sources = []
        for y in range(h):
            for x in range(w):
                address = index_address(x, y, buffer_width)
                if depth == 8:
                    index = packed[address]
                else:
                    index = (packed[address // 2] >> ((address & 1) * 4)) & 15
                entry = clut_index(index, depth)
                rgba += palette[entry * 4:entry * 4 + 4]
                indices.append(index)
                if not level:
                    sources.append([image['offset'] + address * depth // 8,
                                    (address & 1) * 4 if depth == 4 else 0,
                                    palette_base + entry * 4])
        if len(rgba) != w * h * 4:
            raise Unsupported('palette entry out of range')
        images.append({'level': level, 'width': w, 'height': h,
                       'rgba_gs': bytes(rgba), 'indices': bytes(indices),
                       'buffer_width': buffer_width,
                       'transfer_width': transfer['transfer_width'],
                       'transfer_height': transfer['transfer_height'],
                       'destination': {'dsax': transfer['dsax'], 'dsay': transfer['dsay']},
                       'source': dict(image)})
    top = images[0]
    return {'width': top['width'], 'height': top['height'],
            'rgba_gs': top['rgba_gs'], 'indices': top['indices'],
            'texel_sources': sources, 'levels': len(images), 'images': images,
            'padded_upload': top['buffer_width'] != top['width'],
            'clut_destination': {'dsax': record['palette_transfers'][0]['dsax'],
                                 'dsay': record['palette_transfers'][0]['dsay'],
                                 'status': 'RECORDED; runtime GS buffer offset, not interpreted'},
            'colour_policy': 'raw GS RGBA; alpha is nominal 0..128, unconverted'}


PROFILES = [
    {'id': 'PS2_INDEXED_CSM1_V2',
     # Family match is on storage mode alone, so an indexed raster that differs
     # only in version or CLUT mode reports the specific gate it failed instead
     # of vanishing into 'no_registered_profile'.
     'applies': lambda r: r['psm'] in (19, 20),
     'gates': _gates_indexed_csm1,
     'decode': _decode_indexed_csm1,
     'evidence': 'SLUS_212.15 0x4954f0 native reader, 0x4950d8 raster reader; '
                 'GS addressing cross-checked against PCSX2 GSTables and librw ps2raster; '
                 '25,600 texel comparisons in material-slice-verified/source-validation.json. '
                 'Mip and narrow-texture coverage added from texture-survey-1: the 0x8000 '
                 'format bit correlates exactly with MXL >= 1 over 22,791 rasters, the '
                 'upload shape rule predicted all 25,392 stored transfer shapes with no '
                 'misses, and the buffer-width unswizzle stride is the only one of the two '
                 'candidate readings that is collision-free on all 33 shapes present.'},
]


def classify(record):
    """Return (profile_id | None, failed_gate_names). Never guesses a decoder."""
    for profile in PROFILES:
        if profile['applies'](record):
            gates = profile['gates'](record)
            return (profile['id'] if not gates else None), gates
    return None, ['no_registered_profile']


def decode(record):
    """Decode pixels for a registered, fully gated profile. Otherwise fail closed."""
    profile_id, gates = classify(record)
    if profile_id is None:
        raise Unsupported('unsupported raster profile ' + profile_key(record)
                          + ' (' + ','.join(gates) + ')')
    handler = next(p for p in PROFILES if p['id'] == profile_id)
    image = handler['decode'](record)
    image['profile'] = profile_id
    image['reference_render_matched'] = False
    return image


def read_dictionary(dictionary, base=0, want_pixels=True):
    """Full pass over one dictionary: fields, classification and optional pixels."""
    info = texture_chunks(dictionary, base)
    results = []
    for entry in info['textures']:
        kind, at, length = entry['chunk']
        item = {'source': entry['source']}
        try:
            record = texture_fields(take(dictionary, at, length + 12), base + at)
        except Unsupported as exc:
            item.update(status='UNREADABLE', reason=str(exc))
            results.append(item)
            continue
        profile_id, gates = classify(record)
        item.update(name=record['name'], mask=record['mask'],
                    profile_key=profile_key(record), profile=profile_id,
                    failed_gates=gates,
                    fields={k: v for k, v in record.items()
                            if k not in ('pixel_region', 'palette_region')})
        if profile_id and want_pixels:
            item['image'] = decode(record)
        item['status'] = 'DECODABLE' if profile_id else 'UNSUPPORTED_PROFILE'
        results.append(item)
    info.pop('textures')
    return {**info, 'items': results}
