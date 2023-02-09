#include <stdio.h>
#include <stdint.h>
#include <string.h>
#include <stdlib.h>

const uint32_t MIN_SHIFT = 14;
const uint32_t DEPTH = 5;

int reg2bin(int64_t beg, int64_t end, int min_shift, int depth);
int reg2bins(int64_t beg, int64_t end, int min_shift, int depth, int *bins);
int bin_limit(int min_shift, int depth);

int main(int argc, char **argv)
{
    char buf[1000];
    unsigned long long line_num = 0;
    int bin_len = bin_limit(MIN_SHIFT, DEPTH);
    int *bins = (int *)malloc(sizeof(int) * bin_len);

    while (1)
    {
        char *line = fgets(buf, sizeof(buf), stdin);
        if (line == NULL)
        {
            break;
        }
        line_num += 1;

        char *start = strchr(line, '\t');
        if (start == NULL)
        {
            fprintf(stderr, "Fail to parse line at %llu (1)\n", line_num);
            return 1;
        }
        *start = '\0';
        start += 1;

        char *end = strchr(start, '\t');
        if (end == NULL)
        {
            fprintf(stderr, "Fail to parse line at %llu (2)\n", line_num);
            return 1;
        }
        *end = '\0';
        end += 1;

        char *name = strchr(end, '\t');
        if (name == NULL)
        {
            fprintf(stderr, "Fail to parse line at %llu (3)\n", line_num);
            return 1;
        }
        *name = '\0';
        name += 1;

        char *endptr = NULL;
        uint64_t start_pos = strtoull(start, &endptr, 10);
        if (*endptr != '\0')
        {
            fprintf(stderr, "Fail to parse line at %llu (4): %s\n", line_num, endptr);
            return 1;
        }

        endptr = NULL;
        uint64_t end_pos = strtoull(end, &endptr, 10);
        if (*endptr != '\0')
        {
            fprintf(stderr, "Fail to parse line at %llu (5)\n", line_num);
            return 1;
        }

        int bin = reg2bin(start_pos, end_pos, MIN_SHIFT, DEPTH);
        int num_bins = reg2bins(start_pos, end_pos, MIN_SHIFT, DEPTH, bins);

        printf("%s\t%llu\t%llu\t%d\t", line, start_pos, end_pos, bin);
        for (int i = 0; i < num_bins; i++)
        {
            if (i != 0)
            {
                printf(",");
            }
            printf("%d", bins[i]);
        }
        printf("\n");
    }
    return 0;
}

/* calculate bin given an alignment covering [beg,end) (zero-based, half-close-half-open) */
int reg2bin(int64_t beg, int64_t end, int min_shift, int depth)
{
    int l, s = min_shift, t = ((1 << depth * 3) - 1) / 7;
    for (--end, l = depth; l > 0; --l, s += 3, t -= 1 << l * 3)
        if (beg >> s == end >> s)
            return t + (beg >> s);
    return 0;
}
/* calculate the list of bins that may overlap with region [beg,end) (zero-based) */
int reg2bins(int64_t beg, int64_t end, int min_shift, int depth, int *bins)
{
    int l, t, n, s = min_shift + depth * 3;
    for (--end, l = n = t = 0; l <= depth; s -= 3, t += 1 << l * 3, ++l)
    {
        int b = t + (beg >> s), e = t + (end >> s), i;
        for (i = b; i <= e; ++i)
            bins[n++] = i;
    }
    return n;
}
/* calculate maximum bin number -- valid bin numbers range within [0,bin_limit) */
int bin_limit(int min_shift, int depth)
{
    return ((1 << (depth + 1) * 3) - 1) / 7;
}