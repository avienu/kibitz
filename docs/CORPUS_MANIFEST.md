# Corpus manifest

Fingerprints of the git-ignored measurement inputs. Every eval
prints the FNV-64 values below unprompted, so a measurement
self-identifies its inputs; SHA-256 is for verification without
the binary. If a printed fingerprint disagrees with this file at
the same ref, the manifest is stale and the printed value wins.

## Runtime fingerprints (as printed by every eval)
```
input b136b775bc2fc85b  testdata/private/book-trials/amateurs-mind.json (41609 bytes)
input a859cac36ada5b25  testdata/private/book-trials/chess-praxis.json (75347 bytes)
input 9b1287d875a7ff9b  testdata/private/book-trials/chess-strategy.json (50046 bytes)
input e606daf4fda35eb8  testdata/private/book-trials/endgame-course.json (28367 bytes)
input 208733d0c0d3048d  testdata/private/book-trials/htryc.json (44855 bytes)
input 79825c1da72cd7cf  testdata/corpus/quiet_fens.txt (30576 bytes)
inputs-combined 64c3069fcfba3302
```

## SHA-256
```
cc22dafac89d748d53cca0d57a86def6defc4b50f96fd63747a18f9d3385bde7  testdata/private/book-trials/amateurs-mind.json
6b25f6f8fa3a96d8274c0cfa95d0ed973a7abc8a1f7a3ad8bbafdb7047a373f1  testdata/private/book-trials/chess-praxis.json
9a1103ba6c9ec22ecc5a5a5d995cdaf7b484fb4dac417522675255e1f6f77499  testdata/private/book-trials/chess-strategy.json
8cb23bb920b5afe7eb06acca23c3d93b55594acdf2a689a99570bcb2f1382655  testdata/private/book-trials/endgame-course.json
848956b7d26142e85911cdd89095a9252c872ac1386ed6880dd1ddf5ed7bf009  testdata/private/book-trials/htryc.json
4fa5317deb7a7e8e1b23203b6574996bea6095eb08d063365757146d5c4d7a22  testdata/corpus/quiet_fens.txt
```
