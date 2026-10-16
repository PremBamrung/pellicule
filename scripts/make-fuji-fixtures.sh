#!/bin/sh
# Fujifilm MakerNote fixtures for the golden tests in src/meta/fuji.rs.
#
#   docker build --target test -t pellicule:test .
#   docker run --rm -v "$PWD":/src -v "$PWD/samples":/photos:ro -w /src \
#     pellicule:test sh scripts/make-fuji-fixtures.sh
#
# For each photo: the raw MakerNote (no image data) with the camera's internal
# serial number zeroed, and exiftool's reading of it as JSON (raw values, -n).
# exiftool is the oracle only; the parser never uses it.
set -eu

out=tests/fixtures/fuji
mkdir -p "$out"

fixture() {
  name=$1
  photo=/photos/$2
  exiftool -b -MakerNotes "$photo" > "$out/$name.makernote"
  # Tag 0x0010 (InternalSerialNumber): zero its value wherever it is stored.
  perl -e '
    local $/; open my $f, "+<:raw", $ARGV[0] or die; my $b = <$f>;
    substr($b, 0, 8) eq "FUJIFILM" or die "not a Fujifilm MakerNote";
    my $ifd = unpack "V", substr($b, 8, 4);
    my $n = unpack "v", substr($b, $ifd, 2);
    for my $i (0 .. $n - 1) {
      my $e = $ifd + 2 + 12 * $i;
      my ($tag, $type, $count, $value) = unpack "vvVV", substr($b, $e, 12);
      next unless $tag == 0x0010;
      my $at = $count > 4 ? $value : $e + 8;
      substr($b, $at, $count) = "\0" x $count;
    }
    seek $f, 0, 0; print $f $b; close $f;
  ' "$out/$name.makernote"
  exiftool -j -G1 -n -FujiFilm:all -x FujiFilm:InternalSerialNumber "$photo" \
    | sed '/"SourceFile"/d' > "$out/$name.json"
  echo "$name: $(wc -c < "$out/$name.makernote") bytes"
}

fixture xt3-classic-chrome-daylight "Maroc/DSCF3472.JPG"
fixture xt3-classic-chrome-custom "Mariage Thomas/DSCF7199.JPG"
fixture xt3-acros-auto "Maroc/DSCF3466.JPG"
fixture xt3-acros-kelvin "Japon/DSCF0899.JPG"
fixture xe3-acros-kelvin "Japon/DSCF5676_laura.JPG"
fixture xe3-astia "Japon/DSCF6733_laura.JPG"
