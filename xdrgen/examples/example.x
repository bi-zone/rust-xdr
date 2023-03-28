enum Things { A, /* comment a */ B, C /* comment c */ };
struct Bar {
       opaque data<>;
};
typedef Bar BarPair[2];

const VERSION_0 = 0;
struct Foo {
	int a; /* comment a */
	int b;
	int c; /* comment c */
	Bar bar<>;
	BarPair bar_pair;
	Bar *barish;
	string name<>;
	Things thing;
	unsigned type;
};

const VERSION_1 = 1;
union Foobar switch (Things t) {
	case A:
		int a;
	case B:
		string b<>;
	case C:
		Foo c; /* case C */
	default:
		int d; /* default */
};
