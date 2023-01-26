enum Things { A, B, C };
struct Bar {
       opaque data<>;
};
typedef Bar BarPair[2];
struct Foo {
	int a;
	int b;
	int c;
	Bar bar<>;
	BarPair bar_pair;
	Bar *barish;
	string name<>;
	Things thing;
	unsigned type;
};
