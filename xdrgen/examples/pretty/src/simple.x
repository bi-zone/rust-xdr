enum Things { A, /* comment a */ B, C /* comment c */ };
struct Bar {
       opaque data<>;
};
typedef Bar BarPair[2];
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
