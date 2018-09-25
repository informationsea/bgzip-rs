all: bintest_case.txt bintest_case_tbi.txt bincalc

bintest_case.txt: bintest
	./bintest > bintest_case.txt

bintest: bintest.c
	cc -o bintest bintest.c

bintest_case_tbi.txt: bintest_tbi
	./bintest_tbi > bintest_case_tbi.txt

bintest_tbi: bintest_tbi.c
	cc -o bintest_tbi bintest_tbi.c	

bincalc: bincalc.c
	cc -o bincalc bincalc.c

.PHONY: all clean