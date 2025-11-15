"""Inspect the packaged PE and icon resources without loading or launching it."""
import hashlib
import json
import struct
import sys
from pathlib import Path


def verify(path: Path) -> dict:
    data = path.read_bytes()
    assert data[:2] == b'MZ', 'Missing DOS header'
    pe = struct.unpack_from('<I', data, 0x3C)[0]
    assert data[pe:pe + 4] == b'PE\0\0', 'Missing PE header'
    machine, count = struct.unpack_from('<HH', data, pe + 4)
    optional_size = struct.unpack_from('<H', data, pe + 20)[0]
    optional = pe + 24
    assert machine == 0x8664, 'Package must be Windows x64'
    assert struct.unpack_from('<H', data, optional)[0] == 0x20B
    assert struct.unpack_from('<H', data, optional + 68)[0] == 2, 'Package must use the GUI subsystem'
    sections = []
    for i in range(count):
        offset = optional + optional_size + i * 40
        virtual_size, virtual_address, raw_size, raw_offset = struct.unpack_from('<4I', data, offset + 8)
        sections.append((virtual_address, max(virtual_size, raw_size), raw_offset))

    def file_offset(rva):
        for address, size, offset in sections:
            if address <= rva < address + size:
                return offset + rva - address
        raise AssertionError(f'RVA {rva:x} is outside PE sections')

    def c_string(rva):
        offset = file_offset(rva)
        return data[offset:data.index(b'\0', offset)].decode('ascii')

    imports = []
    import_rva = struct.unpack_from('<I', data, optional + 112 + 8)[0]
    if import_rva:
        offset = file_offset(import_rva)
        while any(data[offset:offset + 20]):
            imports.append(c_string(struct.unpack_from('<I', data, offset + 12)[0]))
            offset += 20
    assert not any(name.lower().startswith(('vcruntime', 'msvcp', 'ucrtbase', 'api-ms-win-crt')) for name in imports), 'Dynamic C runtime dependency'

    resource_rva = struct.unpack_from('<I', data, optional + 112 + 16)[0]
    resource_base = file_offset(resource_rva)

    def directory(relative):
        offset = resource_base + relative
        named, ids = struct.unpack_from('<HH', data, offset + 12)
        result = {}
        for i in range(named + ids):
            name, target = struct.unpack_from('<II', data, offset + 16 + i * 8)
            if name & 0x80000000:
                continue
            result[name] = target
        return result

    def resource(kind, resource_id=None):
        target = directory(0)[kind]
        resources = directory(target & 0x7FFFFFFF)
        target = resources[resource_id] if resource_id is not None else next(iter(resources.values()))
        while target & 0x80000000:
            target = next(iter(directory(target & 0x7FFFFFFF).values()))
        rva, size = struct.unpack_from('<II', data, resource_base + target)
        offset = file_offset(rva)
        return data[offset:offset + size]

    group = resource(14)
    reserved, icon_type, variants = struct.unpack_from('<HHH', group)
    assert reserved == 0 and icon_type == 1 and variants == 7
    sizes = []
    for i in range(variants):
        width, height, _, _, planes, bits, byte_count, icon_id = struct.unpack_from('<BBBBHHIH', group, 6 + i * 14)
        icon = resource(3, icon_id)
        assert len(icon) == byte_count and planes == 1 and bits == 32
        header, dib_width, dib_height = struct.unpack_from('<Iii', icon)
        assert header == 40 and dib_width == width and dib_height == height * 2
        pixels = icon[40:40 + width * height * 4]
        assert len(pixels) == width * height * 4
        alpha = pixels[3::4]
        assert 0 in alpha and 255 in alpha, 'Icon must have both visible artwork and transparency'
        assert bytes((94, 103, 28, 255)) in pixels, 'Missing original RoomPlanner icon palette'
        sizes.append(width)
    assert sizes == [16, 20, 24, 32, 48, 64, 128]
    assert b'RoomPlanner third-party license notices' in data, 'License notices must be embedded in the single executable'
    digest = hashlib.sha256(data).hexdigest()
    checksum = Path(str(path) + '.sha256').read_text().split()
    assert checksum == [digest, path.name], 'Checksum file does not match the packaged executable'
    return {'file': str(path.resolve()), 'bytes': len(data), 'architecture': 'Windows x64', 'subsystem': 'GUI', 'icon_sizes': sizes, 'static_crt': True, 'sha256': digest, 'imports': imports}


if __name__ == '__main__':
    package = Path(sys.argv[1] if len(sys.argv) > 1 else 'dist/RoomPlanner-v0.1.0.exe')
    print(json.dumps(verify(package), indent=2))
