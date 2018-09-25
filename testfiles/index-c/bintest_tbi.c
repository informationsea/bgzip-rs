#include <stdio.h>
#include <stdint.h>

#define DEFAULT_MIN_SHIFT 14
#define DEFAULT_DEPTH 5

/* calculate bin given an alignment covering [beg,end) (zero-based, half-close-half-open) */
int reg2bin(int beg, int end)
{
    --end;
    if (beg >> 14 == end >> 14)
        return ((1 << 15) - 1) / 7 + (beg >> 14);
    if (beg >> 17 == end >> 17)
        return ((1 << 12) - 1) / 7 + (beg >> 17);
    if (beg >> 20 == end >> 20)
        return ((1 << 9) - 1) / 7 + (beg >> 20);
    if (beg >> 23 == end >> 23)
        return ((1 << 6) - 1) / 7 + (beg >> 23);
    if (beg >> 26 == end >> 26)
        return ((1 << 3) - 1) / 7 + (beg >> 26);
    return 0;
}

/* calculate the list of bins that may overlap with region [beg,end) (zero-based) */
#define MAX_BIN (((1 << 18) - 1) / 7)
int reg2bins(int rbeg, int rend, uint16_t list[MAX_BIN])
{
    int i = 0, k;
    --rend;
    list[i++] = 0;
    for (k =
             1 + (rbeg >> 26);
         k <=
         1 +
             (rend >> 26);
         ++k)
        list[i++] =
            k;
    for (k =
             9 + (rbeg >> 23);
         k <=
         9 +
             (rend >> 23);
         ++k)
        list[i++] =
            k;
    for (k =
             73 + (rbeg >> 20);
         k <=
         73 +
             (rend >> 20);
         ++k)
        list[i++] =
            k;
    for (k = 585 + (rbeg >> 17); k <= 585 +
                                          (rend >> 17);
         ++k)
        list[i++] =
            k;
    for (k = 4681 + (rbeg >> 14); k <= 4681 +
                                           (rend >> 14);
         ++k)
        list[i++] =
            k;
    return i; // #elements in list[]
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

                        int bin = reg2bin(start, end);
                        uint16_t bins[MAX_BIN];
                        int bins_size = reg2bins(start, end, bins);

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
