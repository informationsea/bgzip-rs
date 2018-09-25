#include <stdio.h>
#include <stdint.h>

#define DEFAULT_MIN_SHIFT 14
#define DEFAULT_DEPTH 5

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

int main()
{
    for (int64_t shift = 13; shift < 18; shift++)
    {
        int64_t base = 1 << shift;
        for (int64_t start_index = 0; start_index < 100; start_index++)
        {
            //printf("%d -> %d\n", shift, start_index);
            int64_t start_base = base * start_index;
            for (int64_t end_index = start_index + 1; end_index < start_index + 20; end_index++)
            {
                //printf("%d -> %d -> %d\n", shift, start_index, end_index);
                int64_t end_base = base * end_index;
                for (int64_t start_border = -2; start_border <= 2; start_border++)
                {
                    int64_t start = start_base + start_border;
                    if (start < 0)
                        continue;
                    for (int64_t end_border = -2; end_border <= 2; end_border++)
                    {
                        int64_t end = end_base + end_border;
                        //printf("%ld %ld %ld %ld / %ld %ld / %ld %ld / %ld %ld\n", base, shift, start_index, end_index, start_base, end_base, start_border, end_border, start, end);

                        int bin = reg2bin(start, end, DEFAULT_MIN_SHIFT, DEFAULT_DEPTH);
                        int bins[(((1 << 18) - 1) / 7)];
                        int bins_size = reg2bins(start, end, DEFAULT_MIN_SHIFT, DEFAULT_DEPTH, bins);

                        printf("%ld\t%ld\t%d", start, end, bin);

                        for (int i = 0; i < bins_size; i++)
                        {
                            printf("\t%d", bins[i]);
                        }
                        printf("\n");
                    }
                }
            }
        }
    }

    return 0;
}
