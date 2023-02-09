#!/usr/bin/env python3

import argparse
import random

CHROMOSOME_LENGTH = [
    248956422, 242193529, 198295559, 190214555, 181538259, 170805979, 159345973,
    145138636, 138394717, 133797422, 135086622, 133275309, 114364328, 107043718, 101991189,
    90338345, 83257441, 80373285, 58617616, 64444167, 46709983, 50818468, 156040895, 57227415
]


def _main():
    parser = argparse.ArgumentParser('Generate test files')
    parser.add_argument('--seed', type=int, default=102335)
    parser.add_argument('--bed-output', default='generated.bed')
    options = parser.parse_args()

    random.seed(options.seed)

    with open(options.bed_output, "wt") as f:
        for chr in range(1, 23):
            start = 0
            for i in range(10000):
                start += int(random.randint(1, int(100 * (1+chr/20)))**2)
                length = int((random.randrange(1, int(300 * chr/5))**2)/2)
                if start + length >= CHROMOSOME_LENGTH[chr - 1]:
                    break
                f.write(
                    f"chr{chr}\t{start}\t{start + length}\tBED_ENTRY_chr{chr}_{i}_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\n")


if __name__ == '__main__':
    _main()
