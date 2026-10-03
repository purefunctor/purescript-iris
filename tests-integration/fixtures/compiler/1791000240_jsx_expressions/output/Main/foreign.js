const optionalComponent = props => props.label ?? props.count ?? null;
export const optional = dictionary => optionalComponent;
