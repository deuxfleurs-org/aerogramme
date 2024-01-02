from os import listdir
from os.path import isfile, join
import sys

path = sys.argv[1]
onlyfiles = [join(path, f) for f in listdir(path) if isfile(join(path, f)) and len(f) > 4 and f[-4:] == ".txt"]

for p in onlyfiles:
    g = p[:-4] + ".eml"
    print(f"{p} -> {g}")
    with open(p, 'r+b') as inp:
        with open(g, 'w+b') as out:
            for line in inp:
                if b"EXPECTED STRUCTURE" in line:
                    break
                out.write(line)
            
