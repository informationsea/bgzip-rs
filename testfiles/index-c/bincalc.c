#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>

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


int main(int argc, char** argv)
{
    if (argc != 3) {
        fprintf(stderr, "%s START STOP\n", argv[0]);
        return 1;
    }

    int start = atoi(argv[1]);
    int stop = atoi(argv[2]);

    int bin = reg2bin(start, stop);
    uint16_t bins[1024];
    int bin_num = reg2bins(start, stop, bins);
    printf("bin = %d\n", bin);
    printf("bins = ");
    for (int i = 0; i < bin_num; i++) {
        if (i != 0) {
            printf(", ");
        }
        printf("%d", bins[i]);
    }
    printf("\n");

    return 0;
}
