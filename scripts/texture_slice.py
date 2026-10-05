"""Bounded PS2 native texture/material slice. Original rendering parity is not claimed."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import struct
import zlib

from load_map import SceneLoader
from inspect_world_geometry import children
from export_map_mesh import single, decode_atomic, batches
from native_geometry_probe import Unsupported, take, words, native_probe


def require(value, message):
    if not value: raise Unsupported(message)


def string(data, chunk):
    _, p, n = chunk
    raw = take(data, p+12, n)
    require(0 < n <= 128 and b'\0' in raw, 'invalid texture string')
    value, padding = raw.split(b'\0', 1)
    require(not any(padding), 'nonzero string padding')
    return value.decode('ascii')


def transfer(data, base):
    """Observed NEWSTYLE GIF A+D TRXPOS/TRXREG/TRXDIR followed by IMAGE."""
    require(words(data, 0, 4) == (3, 0x10000000, 14, 0), 'unsupported GIF register packet')
    regs = {}
    for p, expected in ((16, 0x51), (32, 0x52), (48, 0x53)):
        value, register = struct.unpack('<QQ', take(data, p, 16))
        require(register == expected, 'unexpected transfer register')
        regs[hex(register)] = value
    require(regs['0x53'] == 0, 'transfer is not host to local')
    tag = words(data, 64, 4)
    require(tag[0] <= 0x7fff and tag[1:] == (0x08000000, 0, 0), 'unsupported GIF IMAGE packet')
    length = tag[0]*16
    require(len(data) == 80+length, 'IMAGE size mismatch or additional mip transfers')
    return {'registers': regs, 'source': {'offset': base, 'size': len(data)},
            'pixels_source': {'offset': base+80, 'size': length}}, take(data, 80, length)


def index_address(x, y, width):
    """PS2 RW upload permutation, expressed as bit routing within four rows.

    Reference: aap/librw src/ps2/ps2raster.cpp swizzle/unswizzleRaster.
    Address units are bytes for PAL8 and nibbles for PAL4.
    """
    x2 = ((x >> 2) ^ (y >> 1) ^ (y >> 2)) & 1
    lane = ((y >> 1) & 1) | (((x >> 3) & 1) << 1)
    column = (x & 3) | (x2 << 2) | ((x >> 4) << 3)
    return (y // 4)*width*4 + (y & 1)*width*2 + column*4 + lane


def decode_texture(data, base=0):
    top = single(children(data, 0, len(data)), 0x15)
    require(top[1] == 0 and top[2]+12 == len(data), 'native texture envelope')
    cc = children(data, 12, len(data))
    require([x[0] for x in cc] == [1, 2, 2, 1, 3], 'native texture child profile')
    require(cc[0][2] == 8 and words(data, cc[0][1]+12, 1)[0] == 0x00325350, 'not PS2 native texture')
    addressing = words(data, cc[0][1]+16, 1)[0]
    name, mask = string(data, cc[1]), string(data, cc[2])
    _, p, n = cc[3]
    raster = children(data, p+12, p+12+n)
    require([x[0] for x in raster] == [1, 1] and raster[0][2] == 64, 'native raster structure')
    hp = raster[0][1]+12
    w,h,depth,fmt,version,tex0,paloff,tex1,mip1,mip2,pxsize,palsize,total,tail = struct.unpack('<IIIHHQIIQQIIII', take(data,hp,64))
    require(version == 2 and depth in (4,8) and fmt == (0x4504 if depth == 4 else 0x2504), 'unsupported raster format/version')
    require(32 <= w <= 1024 and 4 <= h <= 1024 and w & (w-1) == 0 and h & (h-1) == 0, 'unsupported dimensions')
    require((tex0 >> 20)&63 == (20 if depth == 4 else 19), 'PSM disagrees with depth')
    require(((tex0 >> 26)&15, (tex0 >> 30)&15) == (w.bit_length()-1,h.bit_length()-1), 'TEX0 dimensions disagree')
    require((tex0 >> 51)&63 == 0 and tex1 == 0, 'unsupported CLUT mode or mip count')
    _, dp, dn = raster[1]
    require(dn == pxsize+palsize, 'native raster payload length mismatch')
    payload = take(data, dp+12, dn)
    pixel_transfer, packed = transfer(take(payload,0,pxsize),base+dp+12)
    palette_transfer, palette = transfer(take(payload,pxsize,palsize),base+dp+12+pxsize)
    require(pixel_transfer['registers']['0x51'] == 0, 'nonzero pixel destination requires GS memory replay')
    require(pixel_transfer['registers']['0x52'] == (w//2)|((h//2)<<32), 'unsupported upload dimensions')
    require(len(packed) == w*h*depth//8, 'packed texel size mismatch')
    # PAL4 NEWSTYLE includes an extra palette row. Preserve it but only address 16 entries.
    require(len(palette) == (96 if depth == 4 else 1024), 'unsupported palette footprint')
    require(palette_transfer['registers']['0x52'] == (8|(3<<32) if depth==4 else 16|(16<<32)), 'palette transfer shape')
    rgba = bytearray(); offsets = []; indices = bytearray()
    for y in range(h):
        for x in range(w):
            address = index_address(x,y,w)
            index = packed[address] if depth==8 else (packed[address//2] >> ((address&1)*4))&15
            ci = (index & ~24)|((index&8)<<1)|((index&16)>>1) if depth==8 else index
            rgba.extend(palette[ci*4:ci*4+4]); indices.append(index)
            offsets.append([pixel_transfer['pixels_source']['offset']+address*depth//8,
                            (address&1)*4 if depth==4 else 0,
                            palette_transfer['pixels_source']['offset']+ci*4])
    return {'name':name,'mask':mask,'width':w,'height':h,'depth':depth,'filter_addressing':addressing,
            'source':{'offset':base,'size':len(data)},'header_source':{'offset':base+hp,'size':64},
            'native_header_hex':take(data,hp,64).hex(),'tex0':tex0,'tex1low':tex1,'miptbp1':mip1,'miptbp2':mip2,
            'palette_offset':paloff,'allocation_size':total,'header_tail_raw':tail,
            'pixel_transfer':pixel_transfer,'palette_transfer':palette_transfer,
            'rgba_gs':bytes(rgba),'indices':bytes(indices),'texel_sources':offsets,
            'reference_render_matched':False}


def png(path,w,h,rgba):
    def chunk(kind,body): return struct.pack('>I',len(body))+kind+body+struct.pack('>I',zlib.crc32(kind+body))
    scan=b''.join(b'\0'+rgba[y*w*4:(y+1)*w*4] for y in range(h))
    path.write_bytes(b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',w,h,8,6,0,0,0))+chunk(b'IDAT',zlib.compress(scan))+chunk(b'IEND',b''))


def export(loader, output):
    require(not output.exists(), 'output already exists')
    part=loader.parts[6868]; atomic=part['atomics'][0]
    def read(span): loader.wad.seek(span['offset']); return loader.wad.read(span['size'])
    geometry,extension=read(atomic['geometry']),read(atomic['extension'])
    gb,eb=atomic['geometry']['offset'],atomic['extension']['offset']
    decoded=decode_atomic(geometry,extension,gb,eb)
    # Membership uses the established world-name lookup, not archive adjacency.
    world=loader.load_world('level100d')
    if not any(x['part_member']==6868 for x in world['instances']):
        raise Unsupported('selected atomic is not in the expected reference world')
    require(world['source']['member_index']==6871, 'reference world identity changed')
    td=world['layout']['texture_dictionary']; dictionary=read(td)
    tc=children(dictionary,12,len(dictionary)); header=single(tc,1)
    count,device=struct.unpack('<HH',take(dictionary,header[1]+12,4))
    native=[x for x in tc if x[0]==0x15]
    require(count==len(native) and device==6, 'dictionary count/device mismatch')
    textures={}
    for _,p,n in native:
        tex=decode_texture(take(dictionary,p,n+12),td['offset']+p)
        require(tex['name'] not in textures,'duplicate texture name')
        textures[tex['name']]=tex
    cc=children(geometry,12,len(geometry)); _,mp,mn=single(cc,8)
    mats=[x for x in children(geometry,mp+12,mp+12+mn) if x[0]==7]
    material=[]
    for _,p,n in mats:
        mc=children(geometry,p+12,p+12+n); _,tp,tn=single(mc,6)
        strings=[x for x in children(geometry,tp+12,tp+12+tn) if x[0]==2]
        name=string(geometry,strings[0]); require(name in textures,'unresolved dictionary name')
        _,sp,sn=single(mc,1)
        material.append({'name':name,'source':{'offset':gb+p,'size':n+12},
                         'raw_hex':take(geometry,p,n+12).hex(),'struct_hex':take(geometry,sp+12,sn).hex()})
    _,ep,en=single(cc,3); plugins=children(geometry,ep+12,ep+12+en)
    _,bp,bn=single(plugins,0x50e); _,np,nn=single(plugins,0x510)
    _,meshcount,_=words(geometry,bp+12,3)
    native_geometry=native_probe(take(geometry,np+12,nn),gb+np+12,meshcount,True)
    ac=children(extension,12,len(extension)); _,sp,_=single(ac,0x3f0)
    uvscale=struct.unpack('<f',take(extension,sp+16,4))[0]
    streams={}
    for mesh in native_geometry['meshes']:
        for draw in batches(geometry,gb,mesh['packets']):
            for i in range(draw['count']):
                ps=draw['inputs'][0]
                streams[ps['source']['offset']+i*8]=(draw,i)
    corners=[]
    for tri,source in zip(decoded['triangles'],decoded['primitive_sources']):
        for pos,off in zip(tri,source['vertex_offsets']):
            draw,i=streams[off]; raw={str(k):v['raw_vectors'][i] for k,v in draw['inputs'].items()}
            uv=raw['1']; require(len(uv)==2,'slice requires V2_16 UVs')
            corners.append({'position':pos,'uv':[v*uvscale for v in uv], 'raw_inputs':raw,
                'sources':{str(k):{'offset':s['source']['offset']+i*(8 if k==0 else 4),'size':8 if k==0 else 4} for k,s in draw['inputs'].items()},
                'material':source['material']})
    output.mkdir(parents=True); (output/'textures').mkdir()
    (output/'native-state').mkdir()
    for name,blob in (('dictionary.bin',dictionary),('geometry.bin',geometry),('atomic-extension.bin',extension)):
        (output/'native-state'/name).write_bytes(blob)
    for name,tex in textures.items():
        require(name.replace('_','').isalnum(),'unsafe texture filename')
        raw=tex.pop('rgba_gs'); indices=tex.pop('indices')
        (output/'textures'/f'{name}.gs.rgba').write_bytes(raw)
        (output/'textures'/f'{name}.indices').write_bytes(indices)
        # Inspection media uses GS nominal 0..128 alpha, retaining untouched raw values.
        rgba=bytearray(raw)
        for i in range(3,len(rgba),4): rgba[i]=min(255,round(rgba[i]*255/128))
        png(output/'textures'/f'{name}.png',tex['width'],tex['height'],rgba)
        tex['image']=f'textures/{name}.png';tex['image_sha256']=hashlib.sha256((output/tex['image']).read_bytes()).hexdigest()
        tex['alpha_export_policy']='min(1, raw_alpha / 128); inspection only; original GS alpha retained'
    report={'schema':'WarriorsMaterialSlice/0.1','source_identities':loader.identities,'part_member':6868,
            'world_member':6871,'world_name':'level100d','materials':material,'textures':textures,'corners':corners,
            'uv_scale':uvscale,'uv_scale_source':{'offset':eb+sp+16,'size':4},
            'instance':next(x for x in world['instances'] if x['part_member']==6868 and x['slot']==atomic['sector_slot']),
            'native_sources':{'dictionary':td,'geometry':atomic['geometry'],'extension':atomic['extension']},
            'coordinate_policy':'Unconverted native local coordinates; physical scale unresolved',
            'reference_render_matched':False,'environment_reconstructed':False,
            'scope':'Textured static inspection; original lighting, fog, GS blending and sampler parity pending'}
    (output/'scene.warriors.json').write_text(json.dumps(report,separators=(',',':')),encoding='utf-8')
    # WMV2 is a private tool format, never interpreted as a game layout.
    with (output/'material.wmv').open('wb') as f:
        f.write(struct.pack('<4sIII',b'WMV2',len(corners),len(material),0))
        first=0
        for m in range(len(material)):
            n=sum(c['material']==m for c in corners)
            require(all(c['material']==m for c in corners[first:first+n]),'noncontiguous material slice')
            f.write(struct.pack('<5I',first,n,6868,0,0));first+=n
        for c in corners:f.write(struct.pack('<3f',*c['position']))
        for c in corners:f.write(struct.pack('<2f',*c['uv']))
        for mat in material:
            t=textures[mat['name']];raw=(output/'textures'/f'{mat["name"]}.gs.rgba').read_bytes()
            rgba=bytearray(raw)
            for i in range(3,len(rgba),4):rgba[i]=min(255,round(rgba[i]*255/128))
            f.write(struct.pack('<II',t['width'],t['height']));f.write(rgba)
    return report


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--output',type=Path,required=True);args=p.parse_args()
    loader=SceneLoader()
    try:
        r=export(loader,args.output);print(json.dumps({'textures':len(r['textures']),'triangles':len(r['corners'])//3,'reference_render_matched':False}))
    finally:loader.close()
