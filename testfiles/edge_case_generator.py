#!/usr/bin/python3

import argparse


def _main():
    parser = argparse.ArgumentParser(
        description="Edge case BED and gff generator")
    parser.add_argument(
        "--bed-output", type=argparse.FileType("w"), default='edge.bed')
    parser.add_argument(
        "--gff-output", type=argparse.FileType("w"), default='edge.gff3')
    options = parser.parse_args()

    MAX_RANGE = 100
    BASE = 1024

    for chr in range(1, 3):
        for i in range(0, MAX_RANGE):
            for k in range(-1, 2):
                if i*BASE + k < 0:
                    continue
                for j in range(i, MAX_RANGE):
                    for l in range(-1, 2):
                        if i*BASE + k > j * BASE + l:
                            continue
                        print("chr{0}\t{1}\t{2}\trange-{3}-{2}".format(chr, i*BASE + k, j *
                                                                       BASE + l + 1, i*BASE + k + 1), file=options.bed_output)
                        print("chr{0}\tEDGE\tregion\t{1}\t{2}\trange-{3}-{2}\t.\t+".format(chr, 1 + i*BASE + k, j *
                                                                                           BASE + l + 1, i*BASE + k + 1), file=options.gff_output)
                        pass


if __name__ == "__main__":
    _main()
