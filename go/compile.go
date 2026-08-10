// Copyright (c) 2026 Richard Rodger and other contributors, MIT License

package tabnasabnf

// AbnfCompile compiles ABNF source into pure-data jsonic text.
func AbnfCompile(src string, opts *AbnfCompileOptions) (string, error) {
	if opts == nil {
		opts = &AbnfCompileOptions{}
	}
	spec, err := Abnf(src, &AbnfConvertOptions{
		Start: opts.Start, Tag: opts.Tag, Builtins: true, Marks: true,
	})
	if err != nil {
		return "", err
	}
	recognition := true
	if opts.Recognition != nil {
		recognition = *opts.Recognition
	}
	var data map[string]any
	if !recognition {
		data, err = ToPureSpec(spec)
	} else {
		data, err = ToRecognitionSpec(spec)
	}
	if err != nil {
		return "", err
	}
	return ToJsonic(data, opts.Strict, opts.Indent), nil
}
