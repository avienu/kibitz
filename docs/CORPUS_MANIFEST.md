# Corpus manifest

Fingerprints of the git-ignored measurement inputs. Every eval
prints the FNV-64 values below unprompted, so a measurement
self-identifies its inputs; SHA-256 is for verification without
the binary. If a printed fingerprint disagrees with this file at
the same ref, the manifest is stale and the printed value wins.

## Runtime fingerprints (as printed by every eval)
```
input b136b775bc2fc85b  testdata/private/book-trials/amateurs-mind.json (41609 bytes)
input e37886e541605661  testdata/private/book-trials/chess-praxis.json (163857 bytes)
input 9b1287d875a7ff9b  testdata/private/book-trials/chess-strategy.json (50046 bytes)
input e606daf4fda35eb8  testdata/private/book-trials/endgame-course.json (28367 bytes)
input 208733d0c0d3048d  testdata/private/book-trials/htryc.json (44855 bytes)
input 61717621afe8e98a  testdata/private/book-trials/kasparov-mgp.json (16028 bytes)
input 88b5c1f7682ffaf1  testdata/private/book-trials/my-system.json (55910 bytes)
input 89b04083ab606098  testdata/private/book-trials/the-blockade.json (14059 bytes)
input 79825c1da72cd7cf  testdata/corpus/quiet_fens.txt (30576 bytes)
inputs-combined 1f26e6f71a276589
```

## SHA-256
```
cc22dafac89d748d53cca0d57a86def6defc4b50f96fd63747a18f9d3385bde7  testdata/private/book-trials/amateurs-mind.json
584ac5358e72bc132a5fac6e982734f97a498a659f12b0f25953814427de1cbf  testdata/private/book-trials/chess-praxis.json
9a1103ba6c9ec22ecc5a5a5d995cdaf7b484fb4dac417522675255e1f6f77499  testdata/private/book-trials/chess-strategy.json
8cb23bb920b5afe7eb06acca23c3d93b55594acdf2a689a99570bcb2f1382655  testdata/private/book-trials/endgame-course.json
848956b7d26142e85911cdd89095a9252c872ac1386ed6880dd1ddf5ed7bf009  testdata/private/book-trials/htryc.json
837b8720071daa2b89d37cecdbcf2b46191933b34a2f4b5481b6b8c80b6cf5c0  testdata/private/book-trials/kasparov-mgp.json
9c67358ffd286e96658769121a00f0d1ce299ba7dadd3dc877f36c62bbedfd98  testdata/private/book-trials/my-system.json
3d9cfd2a938da5ec753fbfb8e51243d16945b6883952010f366fcf9507ec36bf  testdata/private/book-trials/the-blockade.json
4fa5317deb7a7e8e1b23203b6574996bea6095eb08d063365757146d5c4d7a22  testdata/corpus/quiet_fens.txt
```
